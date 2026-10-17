#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A rust-cache step in a target-matrix job keys its cache per target. (#636)

WHAT HAPPENED

Release run 37451381994, job "Build pdfluent (aarch64-unknown-linux-musl)":
`cross build` died running the build scripts of libm, libc and num-traits
with `/lib/x86_64-linux-gnu/libc.so.6: version GLIBC_2.28/2.29/2.30 not
found`. The `Swatinem/rust-cache` step above it had logged `Restored from
cache key "v0-rust-build-Linux-x64-..." full match: true`.

The default cache key has no target in it, so every leg of the `build`
matrix restored the same `target/` -- including host build-script binaries
saved by another leg whose cross image carries a newer glibc. Cargo saw
fresh fingerprints and ran those binaries inside the older aarch64-musl
image. The x86_64 cross legs passed; the cache they restored happened to be
one their image could run.

THE RULE

A job whose strategy matrix has a `target` or `triple` field, and that uses
`Swatinem/rust-cache`, gives that step a `with: key:` that references the
matrix field -- `key: release-binaries-${{ matrix.target }}`. Jobs without a
target matrix keep the default key: legs that only differ by OS already get
different keys from the runner, and there is nothing per-leg to separate.

Exit codes:
  0  every rust-cache step in a target-matrix job keys per target
  1  one of them does not
  2  nothing could be read: no workflow directory, or so few jobs or cache
     steps found that the scan is not looking at what it thinks it is
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

try:
    import yaml
except ImportError:  # pragma: no cover - the lane installs pyyaml
    print("a_rust_cache_key_names_its_target: pyyaml is not installed, "
          "so no workflow could be read.", file=sys.stderr)
    sys.exit(2)

ROOT = pathlib.Path(__file__).resolve().parents[2]

CACHE_ACTION = "Swatinem/rust-cache"

# The matrix fields that name a compile target. A job whose matrix carries
# either one builds once per target into one shared target/ directory.
TARGET_FIELDS = ("target", "triple")


def _matrix_fields(job: dict) -> set[str]:
    """The fields a job's matrix varies, from both matrix spellings.

    `target: [a, b]` names the field directly; `include: [{target: a}]`
    names it per leg. Both put legs with different targets on one cache.
    """
    strategy = job.get("strategy")
    if not isinstance(strategy, dict):
        return set()
    matrix = strategy.get("matrix")
    if not isinstance(matrix, dict):
        return set()
    fields = {k for k in matrix if k not in ("include", "exclude")}
    include = matrix.get("include")
    if isinstance(include, list):
        for leg in include:
            if isinstance(leg, dict):
                fields.update(leg)
    return {str(f) for f in fields}


def _names_field(key: str, field: str) -> bool:
    """Does this cache key reference `matrix.<field>`?

    Both expression spellings count: `matrix.target` and
    `matrix['target']`. A bare `target` elsewhere in the string does not --
    the reference has to be to the matrix, or every leg still restores the
    same key.
    """
    return re.search(r"matrix\s*[\.\[]\s*['\"]?" + re.escape(field) + r"['\"]?",
                     key) is not None


def _jobs(workflows: pathlib.Path):
    """Every (file, job name, job) in the workflow directory."""
    for pattern in ("*.yml", "*.yaml"):
        for path in sorted(workflows.glob(pattern)):
            try:
                document = yaml.safe_load(path.read_text(errors="replace"))
            except yaml.YAMLError as bad:
                yield path, "(unreadable)", {"__error__": str(bad)}
                continue
            if not isinstance(document, dict):
                continue
            jobs = document.get("jobs")
            if not isinstance(jobs, dict):
                continue
            for name, job in jobs.items():
                if isinstance(job, dict):
                    yield path, str(name), job


# A scan that finds nothing approves everything.
MIN_JOBS = 50
MIN_CACHE_USES = 10


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(ROOT),
                        help="the checkout to read (a fixture tree, in the test)")
    arguments = parser.parse_args()
    root = pathlib.Path(arguments.repo).resolve()
    workflows = root / ".github" / "workflows"
    if not workflows.is_dir():
        print(f"a_rust_cache_key_names_its_target: no {workflows}; "
              f"nothing was checked.")
        return 2

    findings: list[str] = []
    seen = 0
    caches = 0
    judged = 0
    for path, name, job in _jobs(workflows):
        if "__error__" in job:
            findings.append(f"{path.name}: could not be parsed: {job['__error__']}")
            continue
        seen += 1
        steps = job.get("steps")
        if not isinstance(steps, list):
            continue
        wanted = [f for f in TARGET_FIELDS if f in _matrix_fields(job)]
        for step in steps:
            if not isinstance(step, dict):
                continue
            if not str(step.get("uses", "")).startswith(CACHE_ACTION):
                continue
            caches += 1
            if not wanted:
                continue
            judged += 1
            with_ = step.get("with")
            key = with_.get("key") if isinstance(with_, dict) else None
            missing = [f for f in wanted
                       if not (isinstance(key, str) and _names_field(key, f))]
            if missing:
                fields = " and ".join(f"matrix.{f}" for f in missing)
                findings.append(
                    f"{path.name}: job `{name}` builds per {fields} but its "
                    f"rust-cache step keys every leg alike"
                    + (f" (key: {key})" if isinstance(key, str) else " (no key:)") + ".")

    if seen < MIN_JOBS or caches < MIN_CACHE_USES:
        print(f"a_rust_cache_key_names_its_target: only {seen} job(s) and "
              f"{caches} rust-cache step(s) found, and there should be at "
              f"least {MIN_JOBS} and {MIN_CACHE_USES}.")
        print("  The scan is not reading the workflows it thinks it is. A scan that finds "
              "nothing approves everything.")
        return 2

    if findings:
        print(f"a_rust_cache_key_names_its_target: {len(findings)} problem(s) "
              f"over {seen} job(s).")
        for finding in findings:
            print(f"  {finding}")
        print("")
        print("A rust-cache step in a target-matrix job keys per target:")
        print("  - uses: Swatinem/rust-cache@v2")
        print("    with:")
        print("      key: <workflow>-${{ matrix.target }}   # or matrix.triple")
        print("Without the field every leg restores the same target/, and a leg runs")
        print("build-script binaries its own image cannot run (#636).")
        return 1

    print(f"a_rust_cache_key_names_its_target: OK — {seen} job(s), {judged} "
          f"rust-cache step(s) in a target matrix, every one keys per target.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
