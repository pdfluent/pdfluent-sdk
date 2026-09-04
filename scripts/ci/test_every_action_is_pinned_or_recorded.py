#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for every_action_is_pinned_or_recorded.py.

On a clean tree this guard finds nothing, and a guard that finds nothing cannot
tell you whether it still recognises anything -- delete its regex and it goes on
passing. So these cases hand it known-bad workflow trees and check that it says
so, and in particular that it says the RIGHT thing: a tag and a branch are
different findings, and pointing a reader at the wrong register is how a
recorded exception ends up covering a case nobody decided on.

The fixture copies the guard into a throwaway tree and runs it there. It reads
`ON_A_TAG` and `ON_A_BRANCH` out of the module rather than repeating them, so
adding a row cannot silently leave the base tree stale -- which is the failure
the two-way check exists to catch, and it would otherwise turn every case here
red at once for the wrong reason.
"""

from __future__ import annotations

import importlib.util
import pathlib
import shutil
import subprocess
import sys
import tempfile

GUARD = pathlib.Path(__file__).with_name("every_action_is_pinned_or_recorded.py")


def _module():
    spec = importlib.util.spec_from_file_location("pinning_guard", GUARD)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


GUARD_MODULE = _module()
FAKE_SHA = "a" * 40

HEAD = "name: {name}\non: {{workflow_dispatch: null}}\njobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n"


def build(root: pathlib.Path, probe: list[str] = [], drop: set[str] = set()) -> None:
    """A tree the guard passes on, plus whatever `probe` adds.

    Every recorded row is used once, because a row nothing uses is itself a
    finding; and the filler clears the `uses:` floor, because below it the guard
    refuses on the scan rather than on the tree and no case here would be
    testing what it says it tests.
    """
    ci = root / "scripts" / "ci"
    ci.mkdir(parents=True)
    shutil.copy(GUARD, ci / GUARD.name)

    flows = root / ".github" / "workflows"
    flows.mkdir(parents=True)

    recorded = sorted(set(GUARD_MODULE.ON_A_TAG) | set(GUARD_MODULE.ON_A_BRANCH))
    lines = [f"      - uses: {key}\n" for key in recorded if key not in drop]
    lines += ["      - uses: actions/checkout@v4\n"] * (GUARD_MODULE.MIN_USES + 10)
    (flows / "base.yml").write_text(HEAD.format(name="Base") + "".join(lines))

    if probe:
        (flows / "probe.yml").write_text(
            HEAD.format(name="Probe") + "".join(f"{line}\n" for line in probe)
        )


def run(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / "scripts" / "ci" / GUARD.name)],
        capture_output=True, text=True, check=False,
    )


CASES: list[tuple[str, dict, bool, str]] = [
    ("a tree in which everything is accounted for", {}, False, ""),

    # The case the guard is for: something new arrives on a ref its owner can
    # move, and nobody wrote down that they meant it to.
    ("a new action on a version tag",
     {"probe": ["      - uses: some/new-action@v1"]}, True, "ON_A_TAG"),

    # Same arrival, worse ref. The message has to name the other register: a
    # branch cannot be excused by the tag list, which is the whole reason there
    # are two.
    ("a new action on a branch",
     {"probe": ["      - uses: some/new-action@main"]}, True, "ON_A_BRANCH"),
    ("a new action on a slash-branch, which looks like a version and is not",
     {"probe": ["      - uses: some/new-action@release/v1"]}, True, "ON_A_BRANCH"),

    # A recorded branch ref may not be laundered through the tag list, nor the
    # other way round.
    ("a recorded branch ref offered as a tag",
     {"probe": ["      - uses: dtolnay/rust-toolchain@v1"]}, True, "ON_A_TAG"),

    # A pin nobody can read is a pin nobody will bump, so it does not count as
    # one.
    ("a commit pin with no version comment",
     {"probe": [f"      - uses: some/new-action@{FAKE_SHA}"]}, True, "no comment"),
    ("a commit pin that says which version it is",
     {"probe": [f"      - uses: some/new-action@{FAKE_SHA} # v1.2.3"]}, False, ""),

    # The other direction. Without this the register only ever grows, and it
    # ends up describing a repository that no longer exists.
    ("a recorded row no workflow uses any more",
     {"drop": {"actions/github-script@v7"}}, True, "no workflow uses it"),

    # docker.yml carries a commented-out `uses:`. Counting it would put a
    # register row on an action nothing runs -- and then the row would be
    # correct until somebody deleted the comment.
    ("a `uses:` inside a comment is an epitaph, not a step",
     {"probe": ["      # - uses: some/new-action@main"]}, False, ""),

    # Unreadable is not the same as fine.
    ("a `uses:` line that names nothing",
     {"probe": ["      - uses:"]}, True, "cannot read"),

    # A scan that finds almost nothing has stopped matching; approving that tree
    # would approve every tree.
    ("a workflow directory the scan barely sees", {"floor": True}, True, "below the floor"),
]


def main() -> int:
    failures = 0
    for name, kwargs, must_fail, needle in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            if kwargs.pop("floor", False):
                ci = root / "scripts" / "ci"
                ci.mkdir(parents=True)
                shutil.copy(GUARD, ci / GUARD.name)
                flows = root / ".github" / "workflows"
                flows.mkdir(parents=True)
                (flows / "thin.yml").write_text(
                    HEAD.format(name="Thin") + "      - uses: actions/checkout@v4\n"
                )
            else:
                build(root, **kwargs)
            result = run(root)
        failed = result.returncode != 0
        output = result.stdout + result.stderr
        if failed != must_fail:
            verb = "passed" if not failed else "refused"
            want = "refuse" if must_fail else "pass"
            print(f"FAIL: {name}: the guard {verb} where it should {want}.")
            print("".join(f"    {line}\n" for line in output.splitlines()[:8]))
            failures += 1
        elif needle and needle not in output:
            print(f"FAIL: {name}: refused, but without saying `{needle}` -- a "
                  "refusal that points at the wrong register is a refusal "
                  "somebody will satisfy in the wrong place.")
            print("".join(f"    {line}\n" for line in output.splitlines()[:8]))
            failures += 1

    if failures:
        print(f"FAIL: {failures} of {len(CASES)} cases.", file=sys.stderr)
        return 1
    print(f"OK: {len(CASES)} cases -- the pinning guard reaches its own verdict.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
