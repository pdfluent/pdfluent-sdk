#!/usr/bin/env python3
"""Every `scripts/ci/test_*.py` is executed by something, or says why not.

Found while fixing #1672, where a check enumerated three file paths by hand
while five files needed covering. This is the same shape one level up: the test
files that police this repository were themselves wired in by hand, one `run:`
line each, and nothing established that a new one runs anywhere at all. A test
file that no job executes is indistinguishable from a test that passes -- and
that difference is what three months of unrun binding tests cost (#307).

So the set is DERIVED from the tree and compared against what the workflows and
the local gate actually RUN.

WHY IT READS COMMANDS AND NOT TEXT

Scanning the raw YAML would count a file named in a comment, in a job `name:`,
or in an epitaph like `# was: python3 scripts/ci/test_x.py`. That is precisely
the defect this guard exists to catch, and `ci_dekking` had it until #1685: the
real step was replaced with `run: true`, the old command left as a comment, and
the guard stayed green. A guard that can be satisfied by a sentence about a
test is not a guard. So only `run`, `script`, `before_script` and `after_script`
are read, and a file that will not parse is an error rather than a fallback to
its bytes.

WHY THE EXCEPTIONS COME FROM THE EXISTING REGISTER

`every_guard_has_a_job.py` already records which guards run nowhere and why,
with the condition for removing each entry. A second list would be a second
answer to one question, which is the defect the register guards exist to catch.
This imports that one.
"""
from __future__ import annotations

import pathlib
import re
import sys

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / "scripts" / "ci"
WORKFLOWS = REPO / ".github" / "workflows"
GITLAB = REPO / ".gitlab-ci.yml"
GATE = CI / "local_ci_gate.sh"

# A search that finds nothing is broken, not clean. Well below the 54 present
# on 03-09-2026, so ordinary deletion does not trip it, but far enough above
# zero to catch a glob that stopped matching.
FLOOR = 25

RUN_KEYS = ("run", "script", "before_script", "after_script")


def command_lines(text: str) -> list[str] | None:
    """What a workflow RUNS, or None if it will not parse.

    Returning None rather than falling back to the raw text is the point: a file
    whose jobs cannot be established is not a file whose jobs are empty.
    """
    try:
        doc = yaml.safe_load(text)
    except yaml.YAMLError:
        return None
    out: list[str] = []

    def walk(node, key=None):
        if isinstance(node, dict):
            for k, v in node.items():
                walk(v, k)
        elif isinstance(node, list):
            for v in node:
                walk(v, key)
        elif isinstance(node, str) and key in RUN_KEYS:
            out.append(node)

    walk(doc)
    return out


def without_comment(line: str) -> str:
    """A shell line with its trailing comment removed, quotes respected.

    `#` only starts a comment at a word boundary, so `sha#deadbeef` and
    `echo "# hi"` survive. Same reader as the one in
    guards_do_not_hide_behind_each_other.py, and for the same reason (#321).
    """
    out: list[str] = []
    quote: str | None = None
    after_space = True
    for ch in line:
        if quote:
            out.append(ch)
            if ch == quote:
                quote = None
        elif ch in ("'", '"'):
            quote = ch
            out.append(ch)
        elif ch == "#" and after_space:
            break
        else:
            out.append(ch)
        after_space = ch.isspace()
    return "".join(out)


def runs(name: str, blob: str) -> bool:
    """Does `name` appear as a whole path, not as the tail of a longer one?

    `test_infra_health.py` contains `infra_health.py`, so a plain substring
    match would report a guard as covered because its namesake runs. The same
    regex as `every_guard_has_a_job._runs`, for the same reason.
    """
    pattern = r"(?<![\w.\-])(?:[\w./\-]*/)?" + re.escape(name) + r"(?![\w.\-])"
    return re.search(pattern, blob) is not None


def recorded_exceptions() -> dict[str, str]:
    """The exceptions already recorded in every_guard_has_a_job.py."""
    sys.path.insert(0, str(CI))
    try:
        from every_guard_has_a_job import ALLOWED
    except Exception as err:  # pragma: no cover - the import is the check
        print(f"[tests-run] SKIPPED (not a pass): could not read the register in "
              f"every_guard_has_a_job.py, so no exception could be honoured: {err}",
              file=sys.stderr)
        raise SystemExit(1)
    return dict(ALLOWED)


def main() -> int:
    files = sorted(p.name for p in CI.glob("test_*.py"))
    if len(files) < FLOOR:
        print(f"[tests-run] FAIL: found {len(files)} test file(s) under "
              f"{CI.relative_to(REPO)}, expected at least {FLOOR}. The search is "
              "broken, not the tree.", file=sys.stderr)
        return 1

    commands: list[str] = []
    for wf in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        lines = command_lines(wf.read_text())
        if lines is None:
            print(f"[tests-run] FAIL: {wf.relative_to(REPO)} does not parse as YAML, "
                  "so what it runs cannot be established.", file=sys.stderr)
            return 1
        commands.extend(lines)

    if GITLAB.exists():
        lines = command_lines(GITLAB.read_text())
        if lines is None:
            print(f"[tests-run] FAIL: {GITLAB.relative_to(REPO)} does not parse as YAML.",
                  file=sys.stderr)
            return 1
        commands.extend(lines)

    if GATE.exists():
        commands.extend(without_comment(r) for r in GATE.read_text().splitlines())

    blob = "\n".join(commands)
    uitzondering = recorded_exceptions()

    uncovered: list[str] = []
    with_reason: list[tuple[str, str]] = []
    for name in files:
        if runs(name, blob):
            continue
        if name in uitzondering:
            with_reason.append((name, uitzondering[name]))
            continue
        uncovered.append(name)

    # An exception that is no longer needed is a register entry that outlives
    # its subject -- the shape this repository keeps finding. Say so.
    stale = [n for n, _ in with_reason if runs(n, blob)]
    for name in sorted(uitzondering):
        if name.startswith("test_") and name in files and runs(name, blob):
            stale.append(name)

    print(f"[tests-run] {len(files)} test file(s), "
          f"{len(files) - len(uncovered) - len(with_reason)} run by a workflow or the "
          f"local gate, {len(with_reason)} recorded as not running")

    if stale:
        print(f"\nFAIL: {len(stale)} file(s) run now and are still recorded as not "
              f"running: {', '.join(sorted(set(stale)))}. Remove the entry from "
              "ALLOWED in every_guard_has_a_job.py so the next one that stops running "
              "is noticed.", file=sys.stderr)
        return 1

    if uncovered:
        print(f"\nFAIL: {len(uncovered)} test file(s) are executed by nothing:\n",
              file=sys.stderr)
        for name in uncovered:
            print(f"  - {name}", file=sys.stderr)
        print("\n  A test no job runs is indistinguishable from a test that passes.\n"
              "  Add a `run:` line in .github/workflows/, a line in local_ci_gate.sh,\n"
              "  or -- if it genuinely cannot run yet -- an entry in ALLOWED in\n"
              "  every_guard_has_a_job.py saying why and what would remove it.",
              file=sys.stderr)
        return 1

    for name, reason in with_reason:
        print(f"  recorded as not running: {name} -- {reason}")
    print("[tests-run] OK: every test file is executed by something, or says why not.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
