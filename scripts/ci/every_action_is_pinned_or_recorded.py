#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A third-party action runs with our credentials, so which code it runs is a
decision -- and this file is where that decision is written down.

`uses: owner/action@v4` does not name code. It names a tag, and the owner of
that repository can move a tag onto anything at any time. `@master` is weaker
still: it is a branch, so it can change between two runs of the same workflow
without a version being cut anywhere. Whatever the ref points at then runs on
the runner that holds the PyPI token, the crates.io token and the signing key.

Measured on 03-09-2026 (#323): 220 `uses:` lines, one of them pinned to a commit.

WHY THIS IS NOT A REPO-WIDE PIN

Because a partial pin is worse than none. Eighteen of the call sites live in
`ci.yml` and `document-register.yml`, which belong to t1 and t3, and `ci.yml` is
the workflow that gates merges. Pinning the other 202 would leave a repository
that READS as pinned while its merge gate is not -- the false-assurance shape
this codebase keeps finding. And 220 pins with nothing bumping them go stale,
which is how a security fix fails to arrive.

So the owner decision on #323 was the narrow one:

  1. Pin the refs that are BRANCHES, because those are the ones that can change
     under a run with nobody releasing anything. Eight call sites, not 220.
  2. Refuse a NEW action arriving on a moving ref, so the surface cannot widen
     while the decision on the existing tag refs stays open.
  3. Record the existing tag refs rather than leave the decision implied.

This file is 2 and 3. The two registers below -- `ON_A_TAG` and `ON_A_BRANCH`
-- hold today's set in BOTH directions: an action that is in neither cannot
arrive, and a row whose action no workflow uses any more cannot stay. Same shape
as `BEKEND` in `a_gate_that_never_went_green.py`; a register that only grows
describes a repository that no longer exists.

They are two lists and not one because they say different things. `ON_A_TAG` is
"not pinned yet, and here is what that action can do with a run". `ON_A_BRANCH`
is "must not be pinned, and here is why" -- and a single list would let the
first quietly answer for the second.

WHY GITHUB'S OWN ACTIONS ARE ROWS LIKE THE REST

`actions/checkout@v4` gets no silent pass here. It is a tag on a repository we
do not control either, and a trust class that is invisible in the register is a
trust class nobody reviews. It is a row, with the reason on it.

Exit codes:
  0  every `uses:` is pinned to a commit, or recorded here with its reason
  1  an unrecorded action, a stale row, a pin without a readable version, or a
     `uses:` line that could not be read
"""

from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"

# FLOOR: this repository had 212 `uses:` lines on 04-09-2026. A scan that finds
# far fewer has stopped matching rather than found a tidier tree, and an empty
# scan approves everything -- which is the mistake every register guard here
# exists to catch.
MIN_USES = 100

SHA = re.compile(r"^[0-9a-f]{40}$")
# `v1`, `v1.4.1`, `1.94.0`. Anything else -- `master`, `main`, `nightly`,
# `release/v1` -- is a branch as far as this guard is concerned, and a branch
# is the case rule 1 is about.
VERSION_TAG = re.compile(r"^v?\d[\w.+-]*$")

# A `uses:` line, with the trailing comment if there is one. Anchored at the
# start so that a `uses:` inside a comment -- `#   uses: docker/...` in
# docker.yml -- is not read as a step. That line is an epitaph, and counting it
# would put a register row on an action nothing runs.
USES = re.compile(r"^\s*(?:-\s*)?uses:\s*(\S+)\s*(?:#\s*(.*?))?\s*$")
ANY_USES = re.compile(r"^\s*(?:-\s*)?uses:")

# Actions on a version tag. Recorded, not pinned: the decision on #323 was to
# close the branch-ref hole first and leave these visible rather than pin 200
# lines nothing would bump.
#
# The reason on each row is what a reader needs to re-take the decision. "It is
# popular" is not one of them; what the action can do with a run is.
ON_A_TAG: dict[str, str] = {
    "actions/checkout@v4": "GitHub's own, on every job; holds the token that fetches the source",
    "actions/upload-artifact@v4": "GitHub's own; writes build output nobody executes",
    "actions/download-artifact@v4": "GitHub's own; reads back what upload-artifact wrote",
    "actions/setup-python@v5": "GitHub's own; installs an interpreter the guards then run",
    "actions/setup-node@v4": "GitHub's own; installs the toolchain the Node bindings build with",
    "actions/setup-java@v4": "GitHub's own; installs the JDK the JNI suite builds with",
    "actions/cache@v4": "GitHub's own; restores a build directory into the workspace",
    "actions/cache/save@v4": "GitHub's own; the save half of the line above",
    "actions/cache/restore@v4": "GitHub's own; the restore half of the line above",
    "actions/github-script@v7": "GitHub's own; runs inline JavaScript with the workflow token",
    "Swatinem/rust-cache@v2": "restores target/ and ~/.cargo into a job that then compiles it",
    "Cyclenerd/hcloud-github-runner@v1": "creates and destroys the ephemeral runners; holds the Hetzner token",
    "PyO3/maturin-action@v1": "builds the Python wheels that are published",
    "peter-evans/repository-dispatch@v3": "fires a dispatch at another repository with a token",
    "softprops/action-gh-release@v2": "uploads the release binaries to the GitHub release",
    "rustsec/audit-check@v2": "reads Cargo.lock and reports advisories; writes nothing",
    "EmbarkStudios/cargo-deny-action@v2": "reads the licence and advisory policy; writes nothing",
    "docker/build-push-action@v6": "builds and pushes the CI snapshot image",
    "docker/login-action@v3": "hands the registry credential to the daemon",
    "docker/metadata-action@v5": "computes tags for the image above",
    "docker/setup-qemu-action@v3": "installs the cross-architecture emulator",
    "docker/setup-buildx-action@v3": "installs the builder the push action drives",
}

# Branch refs that stay branch refs, with the reason. This is the escape hatch
# for rule 1 and it is deliberately small: a row belongs here only when pinning
# would change what the workflow MEANS, not merely which code runs.
#
# Both rows are the same case, and it is a real one. `dtolnay/rust-toolchain`
# has no tags whatsoever -- every ref in it is a branch, `stable` and `nightly`
# included -- and on all 26 of these call sites nothing passes `toolchain:`. So
# the branch name is not a version of the action; it is the argument. Pinning it
# would move the channel choice into a SHA nobody can read, and a bump that
# repointed one of them at `master`, where `toolchain` is `required: true`,
# would break the step outright.
#
# The seven call sites that DID pass `toolchain:` were on `@master`, where the
# ref carries no meaning at all. Those are the ones #323 pinned.
ON_A_BRANCH: dict[str, str] = {
    "dtolnay/rust-toolchain@stable": (
        "the ref is the input, not a version: none of the 23 call sites passes "
        "`toolchain:`, so `stable` is what selects the channel"
    ),
    "dtolnay/rust-toolchain@nightly": (
        "the same case as @stable, on fuzz.yml's three jobs -- cargo-fuzz needs "
        "a nightly compiler and the ref is how that is asked for"
    ),
}


def workflow_files() -> list[pathlib.Path]:
    return sorted(
        [p for p in WORKFLOWS.iterdir() if p.suffix in (".yml", ".yaml")]
    ) if WORKFLOWS.is_dir() else []


def read_uses(path: pathlib.Path) -> tuple[list[tuple[int, str, str]], list[int]]:
    """Every `uses:` in one file: (line number, value, trailing comment).

    The second list is the lines that begin a `uses:` and could not be read.
    They are reported rather than skipped: a line this guard cannot parse is
    indistinguishable, in a silent skip, from a line that is fine.
    """
    found: list[tuple[int, str, str]] = []
    unreadable: list[int] = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not ANY_USES.match(line):
            continue
        m = USES.match(line)
        if not m:
            unreadable.append(n)
            continue
        found.append((n, m.group(1), (m.group(2) or "").strip()))
    return found, unreadable


def main() -> int:
    files = workflow_files()
    if not files:
        print("FAIL: .github/workflows holds no workflow file, so nothing was "
              "checked. That is a broken scan, not a clean tree.", file=sys.stderr)
        return 1

    problems: list[str] = []
    seen: set[str] = set()
    total = 0

    for path in files:
        rel = path.relative_to(REPO)
        found, unreadable = read_uses(path)
        total += len(found) + len(unreadable)
        for n in unreadable:
            problems.append(f"{rel}:{n}: a `uses:` line this guard cannot read. "
                            "Unreadable is not the same as fine.")
        for n, value, comment in found:
            # A step reusing a workflow or an action from this repository runs
            # our own code, which is reviewed here like everything else.
            if value.startswith("./"):
                continue
            if "@" not in value:
                problems.append(f"{rel}:{n}: `{value}` names no ref at all.")
                continue
            action, _, ref = value.rpartition("@")
            if SHA.match(ref):
                if not comment:
                    problems.append(
                        f"{rel}:{n}: `{action}` is pinned to {ref[:12]} with no "
                        "comment saying which version that is. A pin nobody can "
                        "read is a pin nobody will bump."
                    )
                continue
            key = f"{action}@{ref}"
            seen.add(key)
            # A tag row does not excuse a branch ref and vice versa. The two
            # registers say different things -- "not pinned yet" and "must not
            # be pinned" -- and one list would let the first quietly answer for
            # the second.
            if VERSION_TAG.match(ref):
                if key in ON_A_TAG:
                    continue
                where = "ON_A_TAG"
                what = "a moving tag"
            else:
                if key in ON_A_BRANCH:
                    continue
                where = "ON_A_BRANCH"
                what = "a BRANCH, which can change between two runs of the same workflow"
            problems.append(
                f"{rel}:{n}: `{key}` is on {what} and is not recorded. "
                "Pin it to a commit SHA with a `# version` comment, or add it to "
                f"{where} in scripts/ci/every_action_is_pinned_or_recorded.py "
                "with the reason."
            )

    for key in sorted(set(ON_A_TAG) | set(ON_A_BRANCH)):
        if key in seen:
            continue
        problems.append(
            f"scripts/ci/every_action_is_pinned_or_recorded.py: `{key}` is "
            "recorded but no workflow uses it. Remove the row: a register that "
            "only grows describes a repository that no longer exists."
        )

    if total < MIN_USES:
        print(f"FAIL: found {total} `uses:` lines across {len(files)} workflows, "
              f"below the floor of {MIN_USES}. The scan is broken, not the tree.",
              file=sys.stderr)
        return 1

    if problems:
        print(f"FAIL: {len(problems)} action reference(s) unaccounted for.",
              file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        return 1

    print(f"OK: {total} `uses:` lines across {len(files)} workflows -- every one "
          f"pinned to a commit or recorded "
          f"({len(ON_A_TAG)} on a tag, {len(ON_A_BRANCH)} on a branch).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
