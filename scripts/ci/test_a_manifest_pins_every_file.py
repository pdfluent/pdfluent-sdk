#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Break the PDF/A corpus manifests on purpose and check the guard notices.

A guard over twenty thousand documents nobody can eyeball is worth exactly what
its mutations prove. Every case below builds a small but complete corpus in a
temporary directory -- drawn by the real rule, so the halves are the halves the
rule computes -- damages one thing, and requires
`a_manifest_pins_every_file.py` to come back non-zero.

The three the ticket asks for are here by name: a swapped byte, a dropped
licence field, a name that appears in both halves. The others are the failures
that would cost the most if they went unseen: a set that has quietly shrunk, a
document moved between halves by hand, a census written rather than counted, and
a holdout whose seal no longer matches the files it seals.

The last case is the control. Without it every mutation above could be passing
because the fixture itself is broken, which is the same green for a different
reason.

# FLOOR: cases >= 12 -- two-way. Fewer means a mutation was dropped and the suite
# got easier while still reporting green; more without moving this number means
# a case was added and never counted.
"""

from __future__ import annotations

import collections
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pdfa_corpus_rule as rule  # noqa: E402

FLOOR = 12
GUARD = pathlib.Path(__file__).with_name("a_manifest_pins_every_file.py")
PER_HALF = 24
SHARDS = 4

FEATURES = (
    ["fonts_embedded", "images"],
    ["fonts_not_embedded"],
    ["fonts_embedded", "transparency", "non_rgb_colour"],
    ["fonts_embedded", "forms"],
    ["images", "scanned_image_only"],
)


def build(directory: pathlib.Path) -> None:
    """A complete, valid two-half corpus, drawn by the real rule.

    The keys are synthetic and the digests are made up -- nothing here fetches
    anything. What has to be real is the half-split, because the guard
    recomputes it; a fixture that assigned halves by hand would test the fixture.
    """
    directory.mkdir(parents=True, exist_ok=True)
    halves: dict[str, list[dict]] = {"tuning": [], "holdout": []}
    number = 0
    while min(len(halves["tuning"]), len(halves["holdout"])) < PER_HALF:
        key = f"000/{number:06d}.pdf"
        number += 1
        where = rule.half("govdocs1", key)
        if len(halves[where]) >= PER_HALF:
            continue
        size = 20_000 + number * 137
        halves[where].append({
            "source": "govdocs1",
            "key": key,
            "sha256": hashlib.sha256(key.encode()).hexdigest(),
            "bytes": size,
            "shard": "000",
            "offset": 1000 + number,
            "method": 8,
            "csize": size // 2,
            "producer_family": "acrobat_distiller",
            "version": "1.4",
            "size_band": rule.size_band(size),
            "features": FEATURES[number % len(FEATURES)],
        })

    for half, rows in halves.items():
        write_half(directory, half, rows)
    ledger(directory).write_text(ledger_text(directory))


def line(row: dict) -> str:
    return "\t".join([
        row["source"][0], row["key"], row["sha256"], str(row["bytes"]),
        row["shard"], str(row["offset"]), str(row["method"]),
        str(row["csize"]), row["producer_family"], row["version"],
        row["size_band"], ",".join(row["features"]) or "-",
    ])


def write_half(directory: pathlib.Path, half: str, rows: list[dict]) -> None:
    buckets: dict[int, list[str]] = {i: [] for i in range(SHARDS)}
    for row in rows:
        buckets[rule.shard(row["source"], row["key"], SHARDS)].append(line(row))
    files = {}
    for i, lines in buckets.items():
        name = f"pdfa_{half}_entries.{i:02d}.tsv"
        body = "".join(x + "\n" for x in sorted(lines))
        (directory / name).write_text(body)
        files[name] = {"lines": len(lines),
                       "sha256": hashlib.sha256(body.encode()).hexdigest()}
    census: collections.Counter[str] = collections.Counter()
    for row in rows:
        census.update(rule.strata_of(row))
    header = {
        "schema": "pdfluent.pdfa-corpus/1",
        "half": half,
        "rule": {"module": "scripts/ci/pdfa_corpus_rule.py",
                 "version": rule.RULE_VERSION, "seed": rule.SEED,
                 "per_half": rule.PER_HALF,
                 "source_share": rule.SOURCE_SHARE,
                 "stratum_floor": rule.STRATUM_FLOOR},
        "sources": {"govdocs1": {
            "id": "govdocs1", "name": "GovDocs1",
            "home": "https://digitalcorpora.org/corpora/file-corpora/files/",
            "archive_base": "https://example.invalid/fixture/zipfiles",
            "archive_layout": "<archive_base>/<zip>.zip, for the fixture.",
            "licence": "Freely redistributable, for the fixture.",
            "provenance": "A synthetic fixture standing in for GovDocs1, so "
                          "that the guard's own test fetches nothing at all."}},
        "total": len(rows),
        "total_bytes": sum(r["bytes"] for r in rows),
        "entry_files": files,
        "columns": ["source", "key", "sha256", "bytes", "zip", "offset",
                    "method", "csize", "producer_family", "version",
                    "size_band", "features"],
        "strata": dict(sorted(census.items())),
        "strata_below_floor": sorted(
            f"{c} ({census[c]})" for c in rule.floored_cells()
            if census[c] < rule.STRATUM_FLOOR),
    }
    manifest(directory, half).write_text(
        json.dumps(header, indent=2, sort_keys=True) + "\n")


def manifest(directory: pathlib.Path, half: str) -> pathlib.Path:
    return directory / f"PDFA_{half.upper()}_MANIFEST.json"


def ledger(directory: pathlib.Path) -> pathlib.Path:
    return directory / "HOLDOUTS.md"


def seal_of(directory: pathlib.Path, half: str) -> str:
    parts = [manifest(directory, half)]
    parts += sorted(directory.glob(f"pdfa_{half}_entries.*.tsv"))
    h = hashlib.sha256()
    for path in parts:
        h.update(path.name.encode())
        h.update(b"\0")
        h.update(path.read_bytes())
    return h.hexdigest()


def ledger_text(directory: pathlib.Path) -> str:
    total = json.loads(manifest(directory, "holdout").read_text())["total"]
    return ("| draw | drawn | status | documents | seal |\n"
            "|---|---|---|---|---|\n"
            f"| fixture-v1 | 2026-09-07 | sealed | {total} | "
            f"`{seal_of(directory, 'holdout')}` |\n")


# ------------------------------------------------------------------ mutations

def swapped_byte(directory: pathlib.Path) -> str:
    path = sorted(directory.glob("pdfa_tuning_entries.*.tsv"))[0]
    body = path.read_text().splitlines()
    parts = body[0].split("\t")
    parts[2] = ("0" if parts[2][0] != "0" else "1") + parts[2][1:]
    body[0] = "\t".join(parts)
    path.write_text("".join(x + "\n" for x in body))
    return "one byte swapped in one document's digest"


def no_archive_to_fetch_from(directory: pathlib.Path) -> str:
    path = manifest(directory, "holdout")
    doc = json.loads(path.read_text())
    del doc["sources"]["govdocs1"]["archive_base"]
    path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "a source that names no archive to fetch from"


def dropped_licence(directory: pathlib.Path) -> str:
    path = manifest(directory, "tuning")
    doc = json.loads(path.read_text())
    doc["sources"]["govdocs1"]["licence"] = ""
    path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "a source with its licence field emptied"


def duplicated_across_halves(directory: pathlib.Path) -> str:
    """The one failure that cannot be repaired after the fact."""
    tuning = sorted(directory.glob("pdfa_tuning_entries.*.tsv"))[0]
    stolen = tuning.read_text().splitlines()[0]
    target = sorted(directory.glob("pdfa_holdout_entries.*.tsv"))[0]
    body = target.read_text() + stolen + "\n"
    target.write_text(body)
    doc = json.loads(manifest(directory, "holdout").read_text())
    doc["entry_files"][target.name] = {
        "lines": len([x for x in body.splitlines() if x]),
        "sha256": hashlib.sha256(body.encode()).hexdigest()}
    doc["total"] += 1
    manifest(directory, "holdout").write_text(
        json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "one document listed in both the tuning set and the holdout"


def a_shorter_set(directory: pathlib.Path) -> str:
    path = sorted(directory.glob("pdfa_tuning_entries.*.tsv"))[0]
    lines = path.read_text().splitlines()[1:]
    body = "".join(x + "\n" for x in lines)
    path.write_text(body)
    doc = json.loads(manifest(directory, "tuning").read_text())
    doc["entry_files"][path.name] = {
        "lines": len(lines),
        "sha256": hashlib.sha256(body.encode()).hexdigest()}
    doc["total"] -= 1
    manifest(directory, "tuning").write_text(
        json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "a set that has quietly lost a document"


def a_document_moved_between_halves(directory: pathlib.Path) -> str:
    source = sorted(directory.glob("pdfa_holdout_entries.*.tsv"))[0]
    lines = source.read_text().splitlines()
    moved = lines.pop(0)
    source.write_text("".join(x + "\n" for x in lines))
    target = sorted(directory.glob("pdfa_tuning_entries.*.tsv"))[0]
    target.write_text(target.read_text() + moved + "\n")
    for half, path in (("holdout", source), ("tuning", target)):
        doc = json.loads(manifest(directory, half).read_text())
        body = path.read_bytes()
        doc["entry_files"][path.name] = {
            "lines": len([x for x in body.decode().splitlines() if x]),
            "sha256": hashlib.sha256(body).hexdigest()}
        manifest(directory, half).write_text(
            json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "a document moved from the holdout into the tuning set by hand"


def a_written_census(directory: pathlib.Path) -> str:
    path = manifest(directory, "tuning")
    doc = json.loads(path.read_text())
    cell = next(iter(doc["strata"]))
    doc["strata"][cell] = doc["strata"][cell] + 500
    path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "a strata census edited rather than counted"


def a_broken_seal(directory: pathlib.Path) -> str:
    text = ledger(directory).read_text()

    def flip(m: re.Match[str]) -> str:
        digest = m.group(1)
        first = "0" if digest[0] != "0" else "1"
        return f"`{first}{digest[1:]}`"

    ledger(directory).write_text(re.sub(r"`([0-9a-f]{64})`", flip, text))
    return "a holdout whose ledger seal no longer matches the files"


def a_rule_that_moved(directory: pathlib.Path) -> str:
    path = manifest(directory, "tuning")
    doc = json.loads(path.read_text())
    doc["rule"]["seed"] = "some-other-seed"
    path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    return "a corpus drawn with a seed the rule no longer uses"


def restate(directory: pathlib.Path) -> None:
    """Bring both manifests and the ledger back in line with the entry files.

    A mutation that leaves a stale digest behind is red for two reasons, and a
    guard that is red for the wrong one proves nothing about the check under
    test. The two content mutations below therefore re-derive everything that
    is generated -- file digests, totals, census, seal -- so the only thing
    still wrong with the corpus is the thing they set out to plant.
    """
    for half in ("tuning", "holdout"):
        path = manifest(directory, half)
        doc = json.loads(path.read_text())
        rows = []
        for entries in sorted(directory.glob(f"pdfa_{half}_entries.*.tsv")):
            body = entries.read_bytes()
            lines = [x for x in body.decode().splitlines() if x]
            doc["entry_files"][entries.name] = {
                "lines": len(lines),
                "sha256": hashlib.sha256(body).hexdigest()}
            rows += [x.split("\t") for x in lines]
        doc["total"] = len(rows)
        census: collections.Counter[str] = collections.Counter()
        for parts in rows:
            census.update(rule.strata_of({
                "producer_family": parts[8], "version": parts[9],
                "size_band": parts[10],
                "features": [] if parts[11] == "-" else parts[11].split(",")}))
        doc["strata"] = dict(sorted(census.items()))
        doc["strata_below_floor"] = sorted(
            f"{c} ({census[c]})" for c in rule.floored_cells()
            if census[c] < rule.STRATUM_FLOOR)
        path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    ledger(directory).write_text(ledger_text(directory))


def _retype(path: pathlib.Path, index: int, digest: str) -> None:
    lines = path.read_text().splitlines()
    parts = lines[index].split("\t")
    parts[2] = digest
    lines[index] = "\t".join(parts)
    path.write_text("".join(x + "\n" for x in lines))


def the_same_bytes_in_both_halves(directory: pathlib.Path) -> str:
    """Two names, one document, one of them inside the sealed half.

    Nothing here is a duplicate row: both keys are the ones the rule puts in
    the half they sit in, and every generated field is restated. Only the bytes
    are shared -- which is exactly what upstream does to us, and exactly what a
    check on names cannot see.
    """
    tuning = sorted(directory.glob("pdfa_tuning_entries.*.tsv"))[0]
    borrowed = tuning.read_text().splitlines()[0].split("\t")[2]
    _retype(sorted(directory.glob("pdfa_holdout_entries.*.tsv"))[0], 0,
            borrowed)
    restate(directory)
    return ("one document in the tuning set and the holdout at once, under two "
            "names")


def the_same_bytes_twice_in_one_half(directory: pathlib.Path) -> str:
    """One document counted twice in the same half, under two names."""
    path = sorted(directory.glob("pdfa_tuning_entries.*.tsv"))[0]
    borrowed = path.read_text().splitlines()[0].split("\t")[2]
    _retype(path, 1, borrowed)
    restate(directory)
    return "one document counted twice in the same half, under two names"


MUTATIONS = (
    swapped_byte,
    dropped_licence,
    no_archive_to_fetch_from,
    duplicated_across_halves,
    a_shorter_set,
    a_document_moved_between_halves,
    a_written_census,
    a_broken_seal,
    a_rule_that_moved,
    the_same_bytes_in_both_halves,
    the_same_bytes_twice_in_one_half,
)

# What a mutation must be red ABOUT. Only the two content cases carry one: they
# are the cases where a guard could plausibly be red for a neighbouring reason,
# and the point of them is the reason.
EXPECT = {
    the_same_bytes_in_both_halves: "under different names, byte for byte",
    the_same_bytes_twice_in_one_half: "carries the same bytes as",
}


def run(directory: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(GUARD), "--corpus", str(directory),
         "--holdouts", str(ledger(directory)), "--per-half", str(PER_HALF)],
        capture_output=True, text=True, check=False)


def main() -> int:
    failures: list[str] = []
    cases = 0

    for mutate in MUTATIONS:
        with tempfile.TemporaryDirectory() as tmp:
            directory = pathlib.Path(tmp) / "pdfa"
            build(directory)
            what = mutate(directory)
            result = run(directory)
            cases += 1
            if result.returncode == 0:
                failures.append(
                    f"the guard passed {what}\n"
                    f"    stdout: {result.stdout.strip()[:400]}")
            elif mutate in EXPECT and EXPECT[mutate] not in result.stderr:
                failures.append(
                    f"the guard rejected {what}, but not for that reason: it "
                    f"never said {EXPECT[mutate]!r}\n"
                    f"    stderr: {result.stderr.strip()[:600]}")

    with tempfile.TemporaryDirectory() as tmp:
        directory = pathlib.Path(tmp) / "pdfa"
        build(directory)
        result = run(directory)
        cases += 1
        if result.returncode != 0:
            failures.append(
                "the control failed: an unmutated corpus must pass, or every "
                "case above is green for the wrong reason\n"
                f"    stderr: {result.stderr.strip()[:800]}")

    if cases < FLOOR:  # FLOOR
        failures.append(
            f"{cases} cases, floor is {FLOOR}. A mutation was dropped; the "
            "suite got easier and still reported green.")
    if cases > FLOOR:  # FLOOR, upward half
        failures.append(
            f"{cases} cases, floor is {FLOOR}. A case was added without moving "
            f"the number; raise FLOOR to {cases} in the same commit.")

    if failures:
        print(f"[pdfa-corpus-guard] FATAL: {len(failures)} problem(s):",
              file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1

    print(f"[pdfa-corpus-guard] OK: {cases - 1} mutations rejected, the "
          "unmutated corpus accepted.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
