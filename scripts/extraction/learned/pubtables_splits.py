#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""PubTables document lists: reading them, and drawing from them, in one place (#638).

Two draws and two trainers all need the same three answers -- which documents
a list file names, which bucket of the PMC id a document sits in, and what an
equal share per bucket is -- and a draw whose bucketing differs from the
measure set's is an era balance nobody can read. So the rule lives here, once.
"""
from __future__ import annotations

import pathlib
import random
import re

# A PMC id is a clock: the number rises with the article's age, so buckets of
# the number are eras. 250,000 ids is the width both draws stratify over.
BUCKET = 250_000

PMC = re.compile(r"PMC(\d+)$")


def require_file(path: pathlib.Path, what: str) -> pathlib.Path:
    """`path`, or a refusal: a draw or a trainer that cannot read its refusal
    list must stop, not draw or train as if the list were empty."""
    if not path.is_file():
        raise SystemExit(
            f"{what} is not there: {path}. Without its refusal lists a run "
            "would put eval documents in a train list; pass the file or run "
            "where it is checked in.")
    return path


def read_ids(path: pathlib.Path) -> list[str]:
    """The ids a list file names: one per line, `#` comments and blank lines
    skipped, in file order."""
    require_file(path, "the list file")
    ids = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        ids.append(line)
    return ids


def read_id_set(path: pathlib.Path) -> set[str]:
    """The same ids as a set, for refusal checks."""
    return set(read_ids(path))


def document_of(page_name: str) -> str:
    """The document a page name carries: `PMC1064095-p0003.pdf` is a page of
    `PMC1064095`, the name the ground truth files it under."""
    stem = page_name[:-4] if page_name.endswith(".pdf") else page_name
    cut = stem.rfind("-p")
    return stem[:cut] if cut > 0 else stem


def pmc_number(pmcid: str) -> int | None:
    """The number of a PMC id, or None when the name is not one."""
    found = PMC.match((pmcid or "").strip())
    return int(found.group(1)) if found else None


def bucket_of(number: int) -> int:
    """The era bucket of a PMC number: absolute, so a bucket means the same
    era in the train draw and in the measure set."""
    return number // BUCKET


def stratified_draw(candidates: list[str], count: int, seed: int,
                    ) -> tuple[list[str], dict[int, tuple[int, int]]]:
    """`count` of `candidates`, stratified over the PMC id (#638).

    Buckets of BUCKET ids, an equal share per bucket: `count // buckets`
    each, the first `count % buckets` buckets (in id order) one more. A
    bucket with too few candidates gives what it has, and the shortfall goes
    to the others in id order. Deterministic for `seed`.

    Returns the drawn ids in id order, and per bucket (drawn, candidates).
    """
    numbered: dict[int, list[tuple[int, str]]] = {}
    for pmcid in candidates:
        number = pmc_number(pmcid)
        if number is None:
            raise SystemExit(
                f"cannot draw {pmcid!r}: not a PMC id. The draw stratifies "
                "over the PMC number, so every candidate must carry one.")
        numbered.setdefault(bucket_of(number), []).append((number, pmcid))
    if not numbered:
        return [], {}
    buckets = sorted(numbered)
    for bucket in buckets:
        numbered[bucket].sort()
    base, extra = divmod(count, len(buckets))
    rng = random.Random(seed)
    drawn: dict[int, list[tuple[int, str]]] = {}
    for at, bucket in enumerate(buckets):
        quota = base + (1 if at < extra else 0)
        drawn[bucket] = rng.sample(numbered[bucket],
                                   min(quota, len(numbered[bucket])))
    shortfall = count - sum(len(picked) for picked in drawn.values())
    left = {bucket: [c for c in numbered[bucket] if c not in set(drawn[bucket])]
            for bucket in buckets}
    while shortfall > 0:
        moved = 0
        for bucket in buckets:
            if shortfall <= 0:
                break
            if not left[bucket]:
                continue
            drawn[bucket].append(left[bucket].pop(
                rng.randrange(len(left[bucket]))))
            shortfall -= 1
            moved += 1
        if not moved:
            break
    picked = sorted((number, pmcid) for bucket in buckets
                    for number, pmcid in drawn[bucket])
    report = {bucket: (len(drawn[bucket]), len(numbered[bucket]))
              for bucket in buckets}
    return [pmcid for _, pmcid in picked], report
