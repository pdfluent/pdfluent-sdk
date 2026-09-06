#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""How much of a forked crate is actually ours? (LC5, #217)

The question has a different answer per crate, and the answer decides the
treatment entirely: barely changed means do not fork at all, take upstream as a
dependency -- a renamed copy of code you have not touched is a liability with no
return. Substantially rewritten means keep it as a declared fork.

WHY THIS DOES NOT MEASURE AGAINST UPSTREAM'S HEAD

The first attempt at this compared against upstream's current main and reported
5 to 89 per cent. That number is unusable: upstream has kept going since the
fork, and its progress counted as our divergence. `hayro-syntax` is at 0.7.2
upstream where we are at 0.5.6.

WHY THE FORK POINT COMES FROM THE REGISTER AND IS NOT GUESSED HERE

An earlier version of this script chose the fork point itself, by taking the
upstream commits that carried our version number and picking the one our sources
resembled most. That method is wrong in a way that looks right: it was 90
commits late for pdf-syntax and 26 for pdf-interpret, always in the optimistic
direction, because a version number sits in a Cargo.toml for months while the
code moves underneath it (#262).

The fork points in `docs/UPSTREAM_FORKS.toml` were established by content --
identical blobs at the claimed commit, differing at the alternatives -- and each
carries its evidence. This reads them. A crate whose fork point is not
established is reported as not measured rather than measured against a guess:
those are different sentences and only one of them is true.

THREE KINDS OF CHANGE, AND ONLY THE FIRST DECIDES ANYTHING

    substantive  real code changes
    renamed      `hayro_x` -> `pdfluent_x`, mechanical
    doc          our own doc comments

WHAT THIS REFUTED

`THIRD_PARTY_LICENSES.txt` calls jbig2, jpeg2000 and ccitt "(retained as-is)",
and #217 reads that as "so we have not touched those". It does not say that:
that line sits in a table of RENAMINGS and means the crate kept its name.
Measured, the least-changed of the three has 147 substantively changed lines.

    HAYRO_CLONE=/path/to/hayro python3 scripts/ci/fork_divergence.py
    ... --check   # write nothing; fail if the committed table has drifted
"""
from __future__ import annotations

import difflib
import os
import pathlib
import re
import subprocess
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REGISTER = REPO / "docs" / "UPSTREAM_FORKS.toml"
OUT = REPO / "docs" / "FORK_DIVERGENCE.md"

UPSTREAM_URL = "https://github.com/LaurenzV/hayro.git"

# Our directory -> the directory the same code has upstream. The register names
# the upstream CRATE; in the hayro repository the crate and the directory share
# a name except for `hayro` itself, which is the root renderer.
UPSTREAM_DIR = {
    "pdf-syntax": "hayro-syntax",
    "pdf-interpret": "hayro-interpret",
    "pdf-render": "hayro",
    "pdf-font": "hayro-font",
    "hayro-ccitt": "hayro-ccitt",
    "hayro-jbig2": "hayro-jbig2",
    "hayro-jpeg2000": "hayro-jpeg2000",
}

# FLOOR: crates measured >= 5. Five of the register's nine entries carry a fork
# point in the hayro repository. Measure fewer and the clone is broken rather
# than a fork having disappeared -- and an empty table reads as "nothing of
# anybody else's".
MIN_MEASURED = 5

RENAME = re.compile(r"\bhayro[_-]")
DOC = re.compile(r"^\s*//[/!]")


def _sealed() -> dict[str, str]:
    """The environment without GIT_*, so a hook's variables cannot redirect us."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def forks() -> list[dict]:
    return tomllib.loads(REGISTER.read_text(encoding="utf-8")).get("fork", [])


def measure(clone: pathlib.Path, ours: str, theirs: str, sha: str):
    """Line counts for one crate against its fork point.

    A file of ours that upstream does not have at that commit counts entirely as
    ours, which is right: it is a file we added.
    """
    source = REPO / "crates" / ours / "src"
    substantive = renamed = docs = total = 0
    for path in sorted(source.rglob("*.rs")):
        mine = path.read_text(encoding="utf-8", errors="ignore").splitlines()
        total += len(mine)
        got = subprocess.run(
            ["git", "-C", str(clone), "show",
             f"{sha}:{theirs}/src/{path.relative_to(source)}"],
            capture_output=True, text=True, env=_sealed(), check=False,
        )
        upstream = got.stdout.splitlines() if got.returncode == 0 else []
        matcher = difflib.SequenceMatcher(None, mine, upstream, autojunk=False)
        for tag, i1, i2, j1, j2 in matcher.get_opcodes():
            if tag == "equal":
                continue
            their_block = "".join(upstream[j1:j2])
            for line in mine[i1:i2]:
                if DOC.match(line):
                    docs += 1
                elif RENAME.search(line) or RENAME.search(their_block):
                    renamed += 1
                else:
                    substantive += 1
    return total, substantive, renamed, docs


def render(rows: list[tuple], unmeasured: list[tuple[str, str]]) -> str:
    lines = [
        "# How much of each forked crate is ours",
        "",
        "<!-- GENERATED by scripts/ci/fork_divergence.py -- do not edit by hand. -->",
        "",
        "LC5 (#217). Measured against the fork point, not against upstream's head:",
        "upstream has kept going since the fork and its progress would count as ours.",
        "",
        "The fork points come from `docs/UPSTREAM_FORKS.toml`, where each was",
        "established by content and carries its evidence (#262). A crate whose fork",
        "point is not established is listed below the table as not measured, rather",
        "than measured against a guess.",
        "",
        "| Our crate | Upstream | Fork point | Lines | Substantive | Renamed | Doc |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: |",
    ]
    for ours, theirs, sha, total, substantive, renamed, docs in rows:
        lines.append(
            f"| `{ours}` | `{theirs}` | `{sha[:9]}` | {total} | "
            f"**{substantive}** | {renamed} | {docs} |"
        )
    lines += [
        "",
        "## What this refutes",
        "",
        "`THIRD_PARTY_LICENSES.txt` calls jbig2, jpeg2000 and ccitt *(retained as-is)*,",
        "and #217 reads that as \"so we have not touched those\". It does not say that:",
        "that line sits in a table of **renamings** and means the crate kept its name.",
        "",
        "Every measured crate carries substantive changes, so none of them is a",
        "candidate for going back to upstream as a plain dependency (LC6). The",
        "mechanical half -- the `hayro_x` to `pdfluent_x` renaming -- is small",
        "everywhere; it is the code changes that are in the way. LC7, declaring the",
        "fork where a reader looks, is the route for all of them (#219).",
        "",
    ]
    if unmeasured:
        lines += ["## Not measured", ""]
        for name, why in unmeasured:
            lines.append(f"- `{name}` — {why}")
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    check = "--check" in sys.argv
    raw = os.environ.get("HAYRO_CLONE", "")
    clone = pathlib.Path(raw) if raw else None
    if clone is None or not (clone / ".git").exists():
        print(
            "SKIPPED (not a pass): no upstream clone. Set HAYRO_CLONE to a full\n"
            f"  clone of {UPSTREAM_URL} -- without upstream's history there is\n"
            "  nothing to measure against, and a number without one says nothing.",
            file=sys.stderr,
        )
        return 0

    rows = []
    unmeasured: list[tuple[str, str]] = []
    for fork in forks():
        ours = fork["onze_crate"]
        theirs = UPSTREAM_DIR.get(ours)
        sha = fork.get("forkpunt")
        if theirs is None:
            unmeasured.append((ours, "a different upstream, not in this clone"))
            continue
        if not sha:
            unmeasured.append(
                (ours, "no fork point established in `docs/UPSTREAM_FORKS.toml`; "
                       "measuring against a guess is what #262 had to undo"))
            continue
        exists = subprocess.run(["git", "-C", str(clone), "cat-file", "-e",
                                 f"{sha}^{{commit}}"],
                                capture_output=True, env=_sealed(), check=False)
        if exists.returncode != 0:
            unmeasured.append((ours, f"the clone does not carry `{sha}`"))
            continue
        rows.append((ours, theirs, sha, *measure(clone, ours, theirs, sha)))

    if len(rows) < MIN_MEASURED:
        print(f"[forkdiv] {len(rows)} crate(s) measured, fewer than {MIN_MEASURED}. "
              "The clone is broken, not the fork gone.", file=sys.stderr)
        for name, why in unmeasured:
            print(f"  {name}: {why}", file=sys.stderr)
        return 1

    text = render(rows, unmeasured)
    if check:
        current = OUT.read_text(encoding="utf-8") if OUT.is_file() else ""
        if current != text:
            print(f"[forkdiv] {OUT.relative_to(REPO)} has drifted from the code.",
                  file=sys.stderr)
            return 1
        print(f"[forkdiv] up to date ({len(rows)} crates).")
        return 0

    OUT.write_text(text, encoding="utf-8")
    print(f"[forkdiv] {len(rows)} forked crate(s) measured -> {OUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
