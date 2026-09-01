#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Every script in `scripts/ci/` must be called by a pipeline job -- and by one
that can still block a merge.

This is the rule from CLAUDE.md, made mechanical: *nothing is done if there is
no test for it that runs in the CI pipeline.* That rule exists because the
alternative went wrong three times, each time the same way -- the test existed
and nothing ran it.

On 25-08-2026 it happened again, inside the guards themselves. A ratchet on how
many site examples fail to compile had no job at all. It existed, nothing ran
it, and it would first have meant something on the day somebody called it by
hand.

There is a second way for a guard to mean nothing, and it is harder to see.
Since 25-08-2026 GitHub is the main remote and GitLab a nightly mirror of it. A
job declared only in `.gitlab-ci.yml` runs after the night, on a copy, against a
branch nobody is merging. Nothing it finds can stop anything. Two of those had
already cost something by the time they were counted: the real PDF/A gate ran on
the mirror while a broken second gate that could not fail stood before the merge
(#286), and the advisory scan sat on a runner without docker for months (#287).

So this file asks three questions, in order:

  1. Does every script have a job somewhere?         -> orphans, below
  2. Does every mirror-only guard have a written     -> mirror_only_guards.toml
     reason for staying on the mirror?
  3. Is the number of mirror-only guards still       -> SPIEGEL_RATEL
     the number written down here?

A script that deliberately does not belong in CI -- a tool run by hand, a
generator -- is listed below with its reason.

Exit codes:
  0  every guard is accounted for
  1  a guard runs nowhere, runs only on the mirror without a reason, or the
     count moved in either direction
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / ".gitlab-ci.yml"
WORKFLOWS = REPO / ".github" / "workflows"
MAP = REPO / "scripts" / "ci"
REGISTER = MAP / "mirror_only_guards.toml"

# FLOOR: scripts found >= 25 -- the directory holds around eighty. If this
# script finds fewer, the search is broken and the directory is not empty; an
# empty scan would approve everything, which is exactly the mistake this file
# exists to catch.
MIN_SCRIPTS = 25

# RATCHET: guards that run only on the mirror -- not more than this, and not
# fewer either.
#
# The name is kept from the version of this file on `chore/test-reachability-gate`,
# where the two-way ratchet was introduced, so that the two converge rather than
# drift apart.
#
# Why equal and not less-than-or-equal: if the number goes down, room has been
# won, and a ratchet that only looks upward lets that room quietly fill again.
# Failing downward costs one line in the commit that books the win and keeps the
# number honest.
#
# 31-08-2026 (#288): 25 -> 12. Thirteen guards got a job in
# .github/workflows/ci.yml; the remaining twelve are in mirror_only_guards.toml
# with the reason they stay.
#
# WHAT THIS NUMBER DOES NOT SAY
# It says "also runs in GitHub Actions", not "an outside contributor sees it".
# The GitHub remote of this repository is private, so none of these guards
# touches a pull request from outside today. That is the gap in #232, and this
# number does not measure it -- it measures the step towards it.
SPIEGEL_RATEL = 12

# The reasons that can justify a place on the mirror. Free text would approve
# every reason, including "later".
REASONS = {"corpus", "runner", "covered-elsewhere", "credentials"}

# Scripts that deliberately have no job, with the reason. A name here without a
# reason should not exist.
ALLOWED: dict[str, str] = {
    "every_guard_has_a_job.py": "this file itself; its own job does name it",

    # Deliberately local: they exist to do something *before* the pipeline.
    "local_ci_gate.sh": (
        "runs the pipeline gates locally before a push; a job for this would "
        "have the pipeline check itself"
    ),

    # Tools a person runs to know something, not a gate that fails.
    "runner_busy_check.sh": (
        "run by hand before a heavy run -- the runner shares one machine with "
        "the corpus measurements"
    ),
    "check_runner_contention.sh": "likewise, hand diagnosis of runner contention",
    "indirect_font_keys_prevalence.py": (
        "a one-off corpus measurement that supported a design decision, not a gate"
    ),
    "qr11_binding_runtime_mapping.sh": "a one-off quality review (QR-11)",

    # Says so itself, in its own first line: "Run by hand, not in CI." It talks
    # to github.com and raw.githubusercontent, reads each source's file list at a
    # pinned commit and downloads every pick. A job for it would put a network
    # fetch of somebody else's repository on the critical path of every build,
    # and the thing it produces -- corpus/CI_CORPUS_MANIFEST.json -- is committed,
    # so the pipeline reads the result rather than re-earning it.
    "build_gate_corpus_manifest.py": (
        "builds the gate-corpus manifest by downloading from upstream suites; run "
        "by hand when the manifest changes, and its output is committed"
    ),

    # Release moment, not every commit.
    "run_release_drift_check.sh": (
        "belongs on a pipeline schedule, not on a push"
    ),
    "qr9_sanitizers.sh": (
        "ASAN/LSAN plus Miri over the memory-safety selection; a release gate, "
        "needs nightly plus rust-src and runs for tens of minutes"
    ),
    "check_large_blobs.sh": (
        "runs as a git hook before a commit, not in the pipeline -- that is the "
        "moment the blob can still be stopped"
    ),

    # Needs something a pipeline checkout does not have.
    "verapdf_cached.py": (
        "a cache layer around veraPDF, not a verdict of its own: it is handed "
        "to the corpus gates with --verapdf-path. Its behaviour is checked by "
        "test_verapdf_cached.py, which runs in promise-guard on GitHub"
    ),
    "infra_health.py": (
        "reads live systems -- the runner list, the Actions budget -- and needs "
        "gh plus a host token for it. The answer is about today's machines and "
        "not about the change under review, so failing a merge on it stops the "
        "wrong person. test_infra_health.py runs in orchestration-guard and "
        "holds it to telling a working machine from a leaked one. OPEN: this "
        "belongs on a schedule (#288)"
    ),
    "mirror_has_not_drifted.py": (
        "compares github/master with origin/master and therefore needs both "
        "remotes in one checkout; an Actions checkout knows only its own, and "
        "then it announces SKIPPED instead of comparing anything -- permanently "
        "skipped is not a gate. It runs on the machine that mirrors, where both "
        "remotes exist. test_mirror_has_not_drifted.py runs in "
        "orchestration-guard and holds it to still biting. OPEN: a schedule with "
        "a GitLab read token could put this before a merge (#288)"
    ),
    "branches_have_a_merge_request.py": (
        "asks about the checked-out branch, and an Actions checkout is a "
        "detached HEAD -- the script then announces SKIPPED and returns 0, every "
        "run. Permanently skipped reports the same green as a pass. It runs in "
        "the local gate, where there is a branch to ask about; "
        "test_branches_have_a_merge_request.py runs in promise-guard and holds "
        "it to counting"
    ),
}


def _gitlab_commands(conf) -> list[str]:
    out: list[str] = []
    for _, job in (conf or {}).items():
        if not isinstance(job, dict):
            continue
        for key in ("script", "before_script", "after_script"):
            lines = job.get(key) or []
            if isinstance(lines, str):
                lines = [lines]
            out.extend(line for line in lines if isinstance(line, str))
    return out


def _actions_commands(conf) -> list[str]:
    """The `run:` lines of every step of every job in a workflow.

    Only `run:` counts, for the same reason as on GitLab: every step carries a
    `name:` that mentions its script, so searching the whole YAML for a filename
    approves a job that is nothing but a heading.
    """
    out: list[str] = []
    for _, job in ((conf or {}).get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        for step in job.get("steps") or []:
            if isinstance(step, dict) and isinstance(step.get("run"), str):
                out.append(step["run"])
    return out


def _runs(name: str, blob: str) -> bool:
    """Does `name` appear in `blob` as a whole path, and not as a tail?

    A plain `name in blob` approves guards that run nowhere: `test_infra_health.py`
    contains `infra_health.py`, so the guard counted as covered the moment its own
    test got a job. The test ran, the guard did not, and the difference was
    invisible. The same shape would undermine every measurement here -- moving a
    guard to GitHub could drag a namesake along that does not run there.

    Hence: no word character, dot or dash on the left (except a `/` with a path
    prefix), and nothing on the right that extends the filename.
    """
    pattern = r"(?<![\w.\-])(?:[\w./\-]*/)?" + re.escape(name) + r"(?![\w.\-])"
    return re.search(pattern, blob) is not None


def _register() -> tuple[dict[str, dict], list[str]]:
    """The written reasons, or the errors explaining why there are none to read."""
    if not REGISTER.exists():
        return {}, [f"{REGISTER.relative_to(REPO)} does not exist"]
    try:
        import tomllib
    except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
        print(
            "SKIPPED (not a pass): tomllib is missing (Python < 3.11), so "
            f"{REGISTER.relative_to(REPO)} was not read and the mirror list was "
            "not checked.",
            file=sys.stderr,
        )
        return {}, ["tomllib is missing"]
    try:
        data = tomllib.loads(REGISTER.read_text())
    except Exception as e:  # TOMLDecodeError, and anything else that fails to read
        return {}, [f"{REGISTER.relative_to(REPO)} is not valid TOML: {e}"]

    out: dict[str, dict] = {}
    errors: list[str] = []
    entries = data.get("guard") or []
    if not isinstance(entries, list) or not entries:
        errors.append(f"{REGISTER.relative_to(REPO)} contains no [[guard]] entry")
        return out, errors
    for entry in entries:
        name = entry.get("script")
        if not name:
            errors.append("a [[guard]] without `script`")
            continue
        for field in ("reason", "why", "issue"):
            if not entry.get(field):
                errors.append(f"{name}: field `{field}` is missing or empty")
        if entry.get("reason") not in REASONS:
            errors.append(
                f"{name}: reason={entry.get('reason')!r} is none of {sorted(REASONS)}"
            )
        if name in out:
            errors.append(f"{name}: appears twice in the register")
        out[name] = entry
    return out, errors


def main() -> int:
    if not CI.exists():
        print(f"SKIPPED (not a pass): {CI} is missing", file=sys.stderr)
        return 0

    # Only what a job actually executes counts.
    #
    # The first version searched the whole YAML for the filename. That is too
    # lenient: every job here carries a comment naming its script, so removing a
    # job left the name in place and the check stayed green.
    import yaml  # PyYAML is in the CI image

    # Both pipelines count, and since 25-08-2026 that is no longer a detail:
    # GitHub is the main remote and GitLab the nightly mirror. Reading only
    # .gitlab-ci.yml reported guards as orphans while GitHub Actions had a job on
    # them -- and, the other way round and worse, approved a guard that runs only
    # on the mirror and therefore stops nothing.
    gitlab = _gitlab_commands(yaml.safe_load(CI.read_text(errors="replace")))
    actions: list[str] = []
    workflows = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    for wf in workflows:
        try:
            actions.extend(
                _actions_commands(yaml.safe_load(wf.read_text(errors="replace")))
            )
        except yaml.YAMLError as e:
            print(f"FAIL: {wf.relative_to(REPO)} is not valid YAML: {e}", file=sys.stderr)
            return 1

    # FLOOR: workflows >= 1 -- without workflows the scan is broken, not the
    # directory empty. Enforced on the shape and not on a number, hence the
    # marker: an empty list is the only state that is wrong here.
    if not workflows:  # FLOOR
        print(
            f"FLOOR: no workflows found in {WORKFLOWS.relative_to(REPO)}. "
            "The search is broken -- this is not a green.",
            file=sys.stderr,
        )
        return 1

    gh = "\n".join(actions)
    gl = "\n".join(gitlab)
    ci = gh + "\n" + gl

    scripts = sorted(
        p for p in MAP.iterdir()
        if p.is_file() and p.suffix in {".py", ".sh"} and not p.name.startswith("_")
    )
    if len(scripts) < MIN_SCRIPTS:
        print(
            f"FLOOR: {len(scripts)} scripts found in {MAP.relative_to(REPO)}, "
            f"expected >= {MIN_SCRIPTS}. The search is broken -- this is not a green.",
            file=sys.stderr,
        )
        return 1

    problems = 0

    orphans = [
        p.name for p in scripts
        if p.name not in ALLOWED and not _runs(p.name, ci)
    ]
    if orphans:
        problems += 1
        print(
            "Guards without a pipeline job. They exist, and nothing runs them:\n",
            file=sys.stderr,
        )
        for name in orphans:
            print(f"  scripts/ci/{name}", file=sys.stderr)
        print(
            "\nPut a job on it in .github/workflows/ or .gitlab-ci.yml, or add the\n"
            "script to ALLOWED with the reason it belongs in a pair of hands.\n\n"
            "The rule from CLAUDE.md: nothing is done if there is no test for it\n"
            "that runs in the CI pipeline. That holds for the checks themselves.",
            file=sys.stderr,
        )

    # A guard that runs only on the mirror stops nothing.
    mirror_only = [
        p.name for p in scripts
        if p.name not in ALLOWED and _runs(p.name, gl) and not _runs(p.name, gh)
    ]
    print(
        f"[guard-jobs] {len(scripts)} script(s), {len(ALLOWED)} deliberately by "
        f"hand, {len(mirror_only)} only on GitLab"
    )

    # The number on its own is bookkeeping. What it is worth stands per guard in
    # mirror_only_guards.toml, and that register is read in both directions: a
    # guard on the mirror without an entry is a decision that was never taken,
    # and an entry without a guard on the mirror is a reason that outlived its
    # situation and now reads as current.
    register, register_errors = _register()
    if register_errors:
        problems += 1
        print(f"\n{REGISTER.relative_to(REPO)} cannot be used:", file=sys.stderr)
        for e in register_errors:
            print(f"  - {e}", file=sys.stderr)

    undeclared = [n for n in mirror_only if n not in register]
    if undeclared:
        problems += 1
        print(
            "\nThese guards run only on the mirror, where a failure stops nothing,\n"
            "and no reason for that is written down:\n",
            file=sys.stderr,
        )
        for name in sorted(undeclared):
            print(f"  scripts/ci/{name}", file=sys.stderr)
        print(
            f"\nEither give it a job in .github/workflows/, or add it to\n"
            f"{REGISTER.relative_to(REPO)} with reason and why.",
            file=sys.stderr,
        )

    stale = sorted(
        n for n in register
        if n not in mirror_only
    )
    if stale:
        problems += 1
        print(
            f"\nEntries in {REGISTER.relative_to(REPO)} that no longer describe "
            "anything:\n",
            file=sys.stderr,
        )
        for name in stale:
            if not (MAP / name).exists():
                print(f"  {name}: the script no longer exists", file=sys.stderr)
            elif name in ALLOWED:
                print(f"  {name}: is in ALLOWED, so it is not a mirror job", file=sys.stderr)
            elif _runs(name, gh):
                print(f"  {name}: now runs in GitHub Actions -- remove the entry", file=sys.stderr)
            else:
                print(f"  {name}: runs on neither pipeline", file=sys.stderr)
        print(
            "\nA reason that stays behind reads as current. Remove it in the same\n"
            "change that made it untrue.",
            file=sys.stderr,
        )

    if mirror_only:
        print(
            "[guard-jobs] only on the mirror, so stopping nothing at a merge:",
            file=sys.stderr,
        )
        for name in sorted(mirror_only):
            reason = register.get(name, {}).get("reason", "NO REASON WRITTEN DOWN")
            print(f"[guard-jobs]   scripts/ci/{name}  ({reason})", file=sys.stderr)

    if len(mirror_only) != SPIEGEL_RATEL:  # RATCHET
        problems += 1
        direction = "more" if len(mirror_only) > SPIEGEL_RATEL else "fewer"
        print(
            f"\n[guard-jobs] {len(mirror_only)} guards run only on the mirror, "
            f"{direction} than the {SPIEGEL_RATEL} written down here.",
            file=sys.stderr,
        )
        if len(mirror_only) > SPIEGEL_RATEL:
            print(
                "One has been added. Put a job on it in .github/workflows/, or\n"
                "raise SPIEGEL_RATEL and add the entry to "
                f"{REGISTER.relative_to(REPO)}\n"
                "saying why this guard belongs on the mirror.",
                file=sys.stderr,
            )
        else:
            print(
                "One has gone, and that is good news that ought to be pinned down:\n"
                f"set SPIEGEL_RATEL to {len(mirror_only)} and remove its entry from\n"
                f"{REGISTER.relative_to(REPO)}, or the room just won can be filled\n"
                "again tomorrow without anything noticing.",
                file=sys.stderr,
            )

    # And the other way round: an exception that no longer exists has to go.
    gone = sorted(n for n in ALLOWED if not (MAP / n).exists())
    if gone:
        problems += 1
        print(
            f"\nThese names are in ALLOWED but no longer exist: {', '.join(gone)}.\n"
            "Remove them, or the list describes a problem that has been solved.",
            file=sys.stderr,
        )

    if problems:
        return 1

    print(
        f"[guard-jobs] every guard has a job; the {len(mirror_only)} on the mirror "
        "each carry a written reason"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
