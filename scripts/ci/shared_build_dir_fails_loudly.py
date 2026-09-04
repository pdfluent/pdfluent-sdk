#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job that builds into the shared directory must check it is there first.

On 27-08-2026 `/mnt/storagebox/cargo-slots` went missing twice. Four jobs died
on `Permission denied (os error 13)`, which sent the diagnosis after ownership
while the cause was a mount. Two others were worse: they stopped existing, and
GitLab cleaned them up as `no_updates_running` almost an hour later, with the
pipeline waiting on something that was already gone (#264).

Either route counts: sourcing `desktop_env.sh`, which validates on the way past,
or running `shared_build_dir_is_there.sh`, which only checks. The second exists
because `desktop_env.sh` appends a slot suffix to CARGO_TARGET_DIR, and a job
that already has one must not get a second.

FLOOR: at least MINIMUM_JOBS jobs must mention the shared directory at all. If
the scan finds almost none, the variable was renamed and this guard is reporting
a clean pipeline because it stopped looking.
"""

from __future__ import annotations

import pathlib
import sys

import yaml

# FLOOR
MINIMUM_JOBS = 8

WORTEL = pathlib.Path(__file__).resolve().parent.parent.parent
PIJPLIJN = WORTEL / ".gitlab-ci.yml"

MARKERS = ("cargo-slots", "CARGO_TARGET_DIR")
CONTROLES = ("desktop_env.sh", "shared_build_dir_is_there.sh")


def uitgevouwen(doc: dict, job: dict) -> str:
    """The job's own text plus one level of `extends`."""
    samen = dict(job)
    ext = job.get("extends")
    if isinstance(ext, str):
        ext = [ext]
    for naam in ext or []:
        basis = doc.get(naam)
        if isinstance(basis, dict):
            for k, v in basis.items():
                samen.setdefault(k, v)
    return yaml.dump(samen)


def main() -> int:
    if not PIJPLIJN.is_file():
        print(f"[shared-build-dir] FATAL: {PIJPLIJN} is missing", file=sys.stderr)
        return 1
    try:
        doc = yaml.safe_load(PIJPLIJN.read_text()) or {}
    except yaml.YAMLError as fout:
        print(f"[shared-build-dir] FATAL: {PIJPLIJN} does not parse: {fout}", file=sys.stderr)
        return 1

    raakt, zonder = [], []
    for naam, job in doc.items():
        if not isinstance(job, dict) or not ("script" in job or "extends" in job):
            continue
        tekst = uitgevouwen(doc, job)
        if not any(m in tekst for m in MARKERS):
            continue
        raakt.append(naam)
        if not any(c in tekst for c in CONTROLES):
            zonder.append(naam)

    if len(raakt) < MINIMUM_JOBS:  # FLOOR
        print(
            f"[shared-build-dir] FATAL: {len(raakt)} job(s) mention the shared build "
            f"directory, floor is {MINIMUM_JOBS}. The marker was probably renamed, and "
            "a guard that finds nothing reports a clean pipeline.",
            file=sys.stderr,
        )
        return 1

    if zonder:
        print(
            f"[shared-build-dir] FATAL: {len(zonder)} of {len(raakt)} job(s) build into "
            "the shared directory without checking it is there:",
            file=sys.stderr,
        )
        for naam in sorted(zonder):
            print(f"  {naam}", file=sys.stderr)
        print(
            "\nAdd `bash scripts/ci/shared_build_dir_is_there.sh` as the first line of "
            "the job's script, or source scripts/ci/desktop_env.sh if the job wants the "
            "slot handling too. Without it the job fails on `Permission denied` forty "
            "minutes in, or stops existing and is cleaned up an hour later. (#264)",
            file=sys.stderr,
        )
        return 1

    print(
        f"[shared-build-dir] OK: all {len(raakt)} job(s) touching the shared build "
        "directory check it first."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
