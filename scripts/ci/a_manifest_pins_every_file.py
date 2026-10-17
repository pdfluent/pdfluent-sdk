#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PDF/A corpus manifests describe one set of bytes, twice, and never twice
the same document.

Two sets of ten thousand documents carry every PDF/A number this project will
publish (#376). Neither set is in the repository -- twenty thousand crawled PDFs
is sixty gigabytes and not ours to redistribute -- so the manifests are the only
thing standing between "we measured on the corpus" and "we measured on whatever
was in that directory". This guard is what makes them worth that.

WHAT IT CHECKS, AND WHAT EACH ONE IS FOR

  1. Every entry carries a SHA-256 and a positive length. A document that
     changed content under the same name would otherwise pass through the whole
     pipeline as the document it used to be.

  2. Every entry resolves, through its source, to a licence and a provenance
     line. #215's rule -- every document this project reads says where it came
     from -- does not stop applying because the document is not tracked.

  3. Neither set is short. A manifest that quietly loses entries measures less
     and still reports green, and the four axes of #376 are ratios: a smaller
     denominator flatters every one of them.

  4. No document is in both halves. This is the one failure that cannot be
     repaired afterwards. A holdout that shares a single document with the
     tuning set is a holdout that has been tuned against, and every number
     measured on it is worth nothing -- not less, nothing, because there is no
     way to say afterwards how much of the result came from the overlap.

  5. Every document sits in the half the rule puts it in. The halves are drawn
     by hashing the key (`pdfa_corpus_rule.half`), so a hand-edited manifest --
     a document moved out of the holdout because it was inconvenient -- does not
     survive recomputation.

  6. The strata census in the header is what the entries actually say, and the
     cells below the floor are exactly the ones listed as below it. A census
     written by hand is a census that agrees with whatever it wants to.

  7. The holdout's seal in benchmarks/HOLDOUTS.md is the seal of the files on
     disk. #365 may not run until the tuning rounds are finished, and the whole
     value of that wait is that nobody could have looked. The seal is what makes
     "nobody looked" checkable rather than promised.

  8. No corpus PDF is tracked in git.

Exit codes:
  0  both manifests describe what they say they describe
  1  they do not
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pdfa_corpus_rule as rule  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
CORPUS = REPO / "corpus" / "pdfa"
HOLDOUTS = REPO / "benchmarks" / "HOLDOUTS.md"

SHA256 = re.compile(r"^[0-9a-f]{64}$")
COLUMNS = 12

SOURCE_LETTER = {"g": "govdocs1", "s": "safedocs"}


def clean_environment() -> dict[str, str]:
    """The caller's environment without GIT_*.

    A pre-push hook exports GIT_DIR and GIT_INDEX_FILE, and a subprocess that
    inherits them acts on the real repository whatever directory it is pointed
    at (scripts/ci/test_no_test_can_touch_the_real_repo.py).
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def seal_of(directory: pathlib.Path, half: str) -> str:
    """One digest over a manifest and every entry file it names.

    Over the bytes on disk rather than over the parsed content, because the
    question the seal answers is "is this the same file nobody has opened", and
    a parser that normalises whitespace would answer a narrower one.
    """
    manifest = directory / f"PDFA_{half.upper()}_MANIFEST.json"
    parts = [manifest]
    parts += sorted(directory.glob(f"pdfa_{half}_entries.*.tsv"))
    h = hashlib.sha256()
    for path in parts:
        h.update(path.name.encode())
        h.update(b"\0")
        h.update(path.read_bytes())
    return h.hexdigest()


def read_entries(directory: pathlib.Path, half: str,
                 complaints: list[str]) -> list[dict]:
    rows = []
    for path in sorted(directory.glob(f"pdfa_{half}_entries.*.tsv")):
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if not line:
                continue
            parts = line.split("\t")
            if len(parts) != COLUMNS:
                complaints.append(
                    f"{path.name}:{number} has {len(parts)} columns, not "
                    f"{COLUMNS}")
                continue
            (letter, key, digest, nbytes, shard, offset, method, csize,
             producer, version, band, features) = parts
            rows.append({
                "file": path.name, "line": number,
                "source": SOURCE_LETTER.get(letter, letter), "key": key,
                "sha256": digest, "bytes": nbytes, "shard": shard,
                "offset": offset, "method": method, "csize": csize,
                "producer_family": producer, "version": version,
                "size_band": band,
                "features": [] if features == "-" else features.split(","),
            })
    return rows


def check_half(directory: pathlib.Path, half: str, per_half: int,
               complaints: list[str]) -> list[dict]:
    manifest = directory / f"PDFA_{half.upper()}_MANIFEST.json"
    if not manifest.is_file():
        complaints.append(f"{manifest.name} is missing; the {half} set is not "
                          "pinned at all")
        return []
    try:
        doc = json.loads(manifest.read_text())
    except json.JSONDecodeError as e:
        complaints.append(f"{manifest.name} is not valid JSON: {e}")
        return []

    sources = doc.get("sources") or {}
    for sid, source in sorted(sources.items()):
        if not (source.get("licence") or "").strip():
            complaints.append(
                f"{manifest.name}: source `{sid}` names no licence (#215); "
                "every document drawn from it is now an unlicensed document")
        if len((source.get("provenance") or "").strip()) < 40:
            complaints.append(
                f"{manifest.name}: source `{sid}` has no usable provenance "
                "line (#215)")
        if not (source.get("archive_base") or "").startswith("https://"):
            complaints.append(
                f"{manifest.name}: source `{sid}` names no archive to fetch "
                "from. The manifest is the only thing that says where these "
                "bytes come from; without it the digests pin a set nobody can "
                "reproduce.")

    declared = doc.get("rule") or {}
    for field, expected in (("seed", rule.SEED),
                            ("version", rule.RULE_VERSION),
                            ("per_half", rule.PER_HALF),
                            ("stratum_floor", rule.STRATUM_FLOOR)):
        if declared.get(field) != expected:
            complaints.append(
                f"{manifest.name}: the manifest was drawn with "
                f"{field}={declared.get(field)!r}, scripts/ci/pdfa_corpus_rule.py "
                f"now says {expected!r}. The rule moved and the corpus did not; "
                "redraw it, or put the rule back.")

    rows = read_entries(directory, half, complaints)

    files = doc.get("entry_files") or {}
    on_disk = {p.name for p in directory.glob(f"pdfa_{half}_entries.*.tsv")}
    for name in sorted(set(files) | on_disk):
        path = directory / name
        if name not in files:
            complaints.append(
                f"{name} sits next to {manifest.name} without being listed in "
                "it; the set on disk is larger than the set that was drawn")
            continue
        if name not in on_disk:
            complaints.append(f"{manifest.name} lists {name}, which is missing")
            continue
        body = path.read_bytes()
        stated = files[name].get("sha256")
        if hashlib.sha256(body).hexdigest() != stated:
            complaints.append(
                f"{name} does not match the digest {manifest.name} records for "
                "it; somebody edited the entries without redrawing the set")
        lines = len([x for x in body.decode().splitlines() if x])
        if lines != files[name].get("lines"):
            complaints.append(
                f"{name} holds {lines} entries, {manifest.name} says "
                f"{files[name].get('lines')}")

    seen: dict[str, str] = {}
    by_content: dict[str, str] = {}
    for row in rows:
        where = f"{row['file']}:{row['line']}"
        if not SHA256.match(row["sha256"]):
            complaints.append(f"{where} ({row['key']}) has no usable sha256")
        if row["source"] not in sources:
            complaints.append(
                f"{where} names source {row['source']!r}, which the manifest "
                "does not declare -- so the document has no licence")
        try:
            nbytes = int(row["bytes"])
        except ValueError:
            nbytes = 0
        if nbytes <= 0:
            complaints.append(f"{where} ({row['key']}) has no byte count")
        elif rule.size_band(nbytes) != row["size_band"]:
            complaints.append(
                f"{where} ({row['key']}) is {nbytes} bytes, which is not the "
                f"{row['size_band']} band it claims")
        if row["producer_family"] not in rule.PRODUCER_FAMILIES:
            complaints.append(
                f"{where} names producer family {row['producer_family']!r}, "
                "which the rule does not define")
        if row["version"] not in rule.PDF_VERSIONS + ("unknown",):
            complaints.append(
                f"{where} names PDF version {row['version']!r}, which the rule "
                "does not define")
        for feature in row["features"]:
            if feature not in rule.RECORDED_FEATURES:
                complaints.append(
                    f"{where} names feature {feature!r}, which the rule does "
                    "not define")
        identity = f"{row['source']}/{row['key']}"
        if identity in seen:
            complaints.append(
                f"{identity} appears twice in the {half} set "
                f"({seen[identity]} and {where})")
        seen[identity] = where
        if row["sha256"] in by_content:
            complaints.append(
                f"{where} ({identity}) carries the same bytes as "
                f"{by_content[row['sha256']]}, under another name. One "
                f"document counted twice moves every stratum in the {half} "
                "set and weights it twice in every ratio drawn from it.")
        else:
            by_content[row["sha256"]] = f"{identity} at {where}"
        drawn = rule.half(row["source"], row["key"])
        if drawn != half:
            complaints.append(
                f"{where}: the rule puts {identity} in the {drawn} half, and "
                f"it is listed in the {half} one. The halves are not editable.")

    if doc.get("total") != len(rows):
        complaints.append(
            f"{manifest.name} says total={doc.get('total')}, the entry files "
            f"hold {len(rows)}")
    if len(rows) < per_half:
        complaints.append(
            f"the {half} set holds {len(rows)} documents, the floor is "
            f"{per_half}. Every axis in #376 is a ratio; a set that has "
            "quietly lost documents flatters all four of them at once.")

    census: collections.Counter[str] = collections.Counter()
    for row in rows:
        census.update(rule.strata_of(row))
    stated_census = doc.get("strata") or {}
    if {k: v for k, v in census.items()} != stated_census:
        missing = sorted(set(census) ^ set(stated_census))
        differs = sorted(k for k in set(census) & set(stated_census)
                         if census[k] != stated_census[k])
        complaints.append(
            f"{manifest.name}: the strata census is not what the entries say "
            f"({len(missing)} cell(s) only on one side, {len(differs)} with a "
            "different count). It is generated, not written.")

    short = sorted(f"{c} ({census[c]})" for c in rule.floored_cells()
                   if census[c] < rule.STRATUM_FLOOR)
    if short != sorted(doc.get("strata_below_floor") or []):
        complaints.append(
            f"{manifest.name}: `strata_below_floor` says "
            f"{doc.get('strata_below_floor')}, the entries say {short}. A "
            "corpus is allowed a thin cell; it is not allowed to under-report "
            "one.")

    return rows


def check_seal(directory: pathlib.Path, ledger: pathlib.Path,
               complaints: list[str]) -> None:
    if not ledger.is_file():
        complaints.append(
            "benchmarks/HOLDOUTS.md is missing. Without the ledger there is no "
            "record of when the holdout was drawn or whether it has been "
            "spent, and #376's whole protocol rests on that record.")
        return
    text = ledger.read_text()
    rows = re.findall(
        r"^\|\s*(\S+)\s*\|\s*([0-9-]+)\s*\|\s*(\w+)\s*\|\s*(\d+)\s*\|"
        r"\s*`([0-9a-f]{64})`\s*\|", text, re.M)
    active = [r for r in rows if r[2].lower() == "sealed"]
    if len(active) != 1:
        complaints.append(
            f"benchmarks/HOLDOUTS.md lists {len(active)} sealed holdout(s); "
            "there is exactly one corpus and it has exactly one seal. A second "
            "sealed row means two answers to the question of which documents "
            "nobody has looked at.")
        return
    _draw_id, _date, _status, count, seal = active[0]
    found = seal_of(directory, "holdout")
    if found != seal:
        complaints.append(
            "benchmarks/HOLDOUTS.md seals the holdout as\n"
            f"    {seal}\n"
            "and the files on disk hash to\n"
            f"    {found}\n"
            "The sealed set is not the set in the tree. Either the holdout was "
            "redrawn without booking it in the ledger -- in which case the "
            "ledger is now fiction -- or the manifest was edited, which is the "
            "same thing said more quietly. A holdout that moves is not a "
            "holdout (#365).")
    manifest = directory / "PDFA_HOLDOUT_MANIFEST.json"
    if manifest.is_file():
        try:
            total = json.loads(manifest.read_text()).get("total")
        except json.JSONDecodeError:
            total = None
        if total is not None and int(count) != total:
            complaints.append(
                f"benchmarks/HOLDOUTS.md seals {count} documents, the manifest "
                f"holds {total}")


def check_nothing_is_tracked(complaints: list[str]) -> None:
    try:
        tracked = subprocess.run(
            ["git", "ls-files", "--", "corpus/pdfa/*.pdf",
             "corpus/pdfa/**/*.pdf"],
            capture_output=True, text=True, check=False, timeout=60,
            cwd=REPO, env=clean_environment()).stdout.split()
    except (OSError, subprocess.SubprocessError):
        return
    if tracked:
        complaints.append(
            f"{len(tracked)} corpus PDF(s) are tracked in git. Twenty thousand "
            "crawled documents are neither ours to redistribute nor something "
            "to carry in a source tree; the manifest exists so they do not "
            "have to be (#215).")


def main() -> int:
    p = argparse.ArgumentParser()
    # A path, so the guard's own test can point it at a deliberately broken
    # copy. A guard nobody has tried to defeat is not a guard.
    p.add_argument("--corpus", default=str(CORPUS))
    p.add_argument("--holdouts", default=str(HOLDOUTS))
    p.add_argument("--per-half", type=int, default=rule.PER_HALF,
                   help="for the guard's own test; CI runs it at the "
                        "rule's own floor")
    args = p.parse_args()
    directory = pathlib.Path(args.corpus)

    complaints: list[str] = []
    halves = {half: check_half(directory, half, args.per_half, complaints)
              for half in ("tuning", "holdout")}

    tuning = {f"{r['source']}/{r['key']}" for r in halves["tuning"]}
    holdout = {f"{r['source']}/{r['key']}" for r in halves["holdout"]}
    both = sorted(tuning & holdout)
    if both:
        complaints.append(
            f"{len(both)} document(s) are in the tuning set AND the holdout: "
            + ", ".join(both[:5])
            + ("…" if len(both) > 5 else "")
            + ". Every number measured on this holdout would be a number "
              "measured partly on documents that were tuned against, and no "
              "amount of arithmetic afterwards can take that back out.")

    # And again on the bytes, because the halves are drawn from names and the
    # same bytes live upstream under more than one name. A document that is in
    # the tuning set under one name and in the holdout under another is not a
    # bookkeeping slip: it is a tuned-on document inside the sealed set, and
    # the identity check above cannot see it.
    tuning_content = {r["sha256"]: f"{r['source']}/{r['key']}"
                      for r in halves["tuning"]}
    holdout_content = {r["sha256"]: f"{r['source']}/{r['key']}"
                       for r in halves["holdout"]}
    shared = sorted(set(tuning_content) & set(holdout_content))
    if shared:
        pairs = ", ".join(f"{tuning_content[d]} = {holdout_content[d]}"
                          for d in shared[:5])
        complaints.append(
            f"{len(shared)} document(s) are in both halves under different "
            f"names, byte for byte: {pairs}"
            + ("…" if len(shared) > 5 else "")
            + ". The holdout is only sealed against the names in it; these "
              "bytes are already in the tuning set.")

    check_seal(directory, pathlib.Path(args.holdouts), complaints)
    check_nothing_is_tracked(complaints)

    if complaints:
        print(f"[pdfa-corpus] FATAL: {len(complaints)} problem(s):",
              file=sys.stderr)
        for complaint in complaints[:30]:
            print(f"  {complaint}", file=sys.stderr)
        if len(complaints) > 30:
            print(f"  … and {len(complaints) - 30} more", file=sys.stderr)
        return 1

    total_bytes = sum(int(r["bytes"]) for rows in halves.values() for r in rows)
    print(f"[pdfa-corpus] OK: {len(halves['tuning'])} tuning + "
          f"{len(halves['holdout'])} holdout documents, no overlap by name or "
          f"by content, "
          f"{total_bytes / 1e9:.1f} GB, every one pinned to a digest and a "
          "licensed source; the holdout matches its seal.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
