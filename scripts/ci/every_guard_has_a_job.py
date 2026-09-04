#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
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
# 02-09-2026 (#307): 12 -> 13. Not a guard moved onto the mirror: a guard that
# had always stood before no merge was counted for the first time. The lines a
# job runs were read without asking whether the job can run on a pull request,
# so `tests_actually_ran.py`, called only from a workflow_dispatch-only
# workflow, counted as "has a job" (codex, #1633). The count now includes
# every guard whose only GitHub job is dispatch-only or parked behind
# `needs: parked`, and that one is in mirror_only_guards.toml with its reason.
#
# 02-09-2026 (#276): 13 -> 14. Not a guard at all: crash-guard.yml's verdict --
# ok, crash or hang -- moved out of the workflow into
# classify_render_outcome.sh so that a test could drive it, and a shell function
# in scripts/ci counts here like everything else. Its caller is parked behind
# `needs: parked`, so it stands before no merge; the suite that drives it does,
# on every pull request. Reason in mirror_only_guards.toml.
#
# WHAT THIS NUMBER DOES NOT SAY
# It says "also runs in GitHub Actions", not "an outside contributor sees it".
# The GitHub remote of this repository is private, so none of these guards
# touches a pull request from outside today. That is the gap in #232, and this
# number does not measure it -- it measures the step towards it.
SPIEGEL_RATEL = 14

# The reasons that can justify a place on the mirror. Free text would approve
# every reason, including "later".
REASONS = {"corpus", "runner", "covered-elsewhere", "credentials"}

# Scripts that deliberately have no job, with the reason. A name here without a
# reason should not exist.
ALLOWED: dict[str, str] = {
    "every_guard_has_a_job.py": "this file itself; its own job does name it",

    # Cannot work in a workflow, by its own measurement: /actions/runners needs
    # `administration: read`, which GITHUB_TOKEN cannot be granted, and there is
    # no `gh` on the runner. Wired into a job it printed SKIPPED and returned 0
    # on every run -- a green tick for a check that never asked anything, while
    # this file counted the filename in a `run:` and called it enforced.
    #
    # It runs in the local gate, where `gh` is authenticated, and it now exits 1
    # rather than 0 when CI is set, so wiring it again fails loudly instead of
    # quietly. A token for it is an owner decision (#290). (codex, #1639)
    "every_label_has_a_runner.py": (
        "needs `gh` with a keyring; GITHUB_TOKEN cannot be granted the scope "
        "/actions/runners requires, so in a workflow it can only skip. Runs in "
        "scripts/ci/local_ci_gate.sh"
    ),

    # Not a guard. A shared helper the fixtures import; a job for it would run
    # a module with no verdict and report success for having imported it. It
    # arrived with #1647 and was counted here as an orphan on every run, which
    # is the register asking a fair question about a file that does not belong
    # in it.
    "fixture_env.py": (
        "the sealed-environment helper the fixtures import, not a check: it "
        "has no verdict of its own, and what it does is proven by "
        "test_a_fixture_cannot_touch_a_real_repo.py"
    ),

    # ---- publication guards, split onto master ahead of their wiring (#222) ----
    # These arrived from t3/1543-resolve so they can be read and reviewed on
    # master. every_document_is_registered.py is no longer among them: the seven
    # documents its PUBLIC_TREE entries name -- which until 02-09-2026 existed
    # only on #1543, so the register stood red on master by decision rather than
    # delete the protective lines -- landed byte-identical from that branch, and
    # the guard runs in .github/workflows/document-register.yml.
    "corpus_herkomst.py": (
        "the provenance register, imported by every_document_is_registered.py "
        "(document-register.yml), which asks herkomst() for every tracked PDF; "
        "its own main() regenerates docs/CORPUS_HERKOMST.md and is run by hand"
    ),
    # The other three measure the tree as it stands and could be wired today; they
    # are held back only so they land and are reviewed as one set.
    # Pre-push only, and that is where it decides: master takes fast-forwards
    # only, so the local gate IS the merge point. Its CI counterpart is the
    # inline "Every commit written since the DCO carries a matching sign-off"
    # step in the same workflow, which asks the same question of a pull
    # request's own commits; a second job running this script would ask it
    # twice. Its test IS wired, in orchestration-guard. (#316)
    "every_commit_since_the_cutoff_is_signed.py": (
        "the pre-push half of the #316 sign-off gate; CI asks the same question "
        "in orchestration-guard's inline step, and this script's test runs there"
    ),
    "geen_interne_zaken.py": "wired with the other publication guards after #1543",
    "internal_stays_internal.py": "wired with the other publication guards after #1543",
    "simulate_public_tree.py": "wired with the other publication guards after #1543",
    "herkomsttabel.py": (
        "generates docs/HERKOMST.md and is imported by header_sweep for the list "
        "of forked crates; wired with the set after #1543"
    ),
    "header_sweep.py": (
        "red on master on purpose: 18 of 630 own source files carry the old "
        "proprietary header and the rest carry none, which the flip (#257) and "
        "the header commit on #1543 fix together"
    ),
    # Diagnostics, not gates. Both answer a question a human is already asking
    # when something has gone wrong; neither has a verdict a pipeline could act
    # on. A job for either would report success for having run.
    "bounded_probe.sh": (
        "touches a filesystem that may be dead, on purpose and with a bound. "
        "It is the tool you reach for when a mount hangs, and it has no pass "
        "or fail of its own"
    ),
    "why_not_writable.sh": (
        "tells a human which of three causes made a path unwritable. It "
        "diagnoses an already-failed gate rather than being one"
    ),

    # Written 02-09-2026 (#307) while ci.yml was frozen for #1666. It belongs in
    # orchestration-guard next to test_gate_corpus_is_pinned.py, and that is a
    # one-line change to make the moment ci.yml is open again; until then it
    # runs by hand: python3 scripts/ci/test_gate_corpus_no_crash.py. Listed
    # here so the debt is visible, not so it is excused -- remove this entry in
    # the change that adds the step.
    "test_gate_corpus_no_crash.py": (
        "holds the no-crash gate to stopping at its budget while running; "
        "awaiting its run: line in ci.yml, which #1666 has frozen (#307)"
    ),

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
    # Maintenance, run when the disk-headroom floor is reached from below --
    # exactly the moment a job must not start. A job for it would delete build
    # caches on a schedule nobody chose. (#298)
    "sweep_merged_build_caches.py": (
        "reclaims target/ from worktrees whose work is already on master; a "
        "maintenance action, run when the floor says there is no room"
    ),
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


# The events on which a workflow's verdict can still stop a merge. A workflow
# that fires on none of these -- `workflow_dispatch` only, or a `schedule` --
# runs when somebody remembers to ask, on whatever ref they name, and its red
# reaches no pull request. A `push` that names only tags is a release moment,
# not a merge, and is treated the same way.
BLOCKING_EVENTS = {"pull_request", "pull_request_target", "push", "merge_group"}

# A job whose `needs` chain passes through a job by this name never runs: the
# parked job exists to fail loudly on a dispatch nobody should be making (#276),
# and everything behind it is a queue entry pretending to be a gate.
PARKED_JOB = "parked"


def _events(conf) -> dict:
    """The `on:` block, keyed by event name. PyYAML reads a bare `on` as the
    boolean True (YAML 1.1), so both spellings are looked up."""
    on = (conf or {}).get("on", (conf or {}).get(True))
    if isinstance(on, str):
        return {on: None}
    if isinstance(on, list):
        return {str(x): None for x in on}
    if isinstance(on, dict):
        return {str(k): v for k, v in on.items()}
    return {}


def _can_block(conf) -> bool:
    """Can a red from this workflow stand in front of a merge at all?"""
    for event, spec in _events(conf).items():
        if event not in BLOCKING_EVENTS:
            continue
        if event == "push" and isinstance(spec, dict) and "tags" in spec and "branches" not in spec:
            continue  # a tag push is a release, not a merge
        return True
    return False


def _needs(job) -> list[str]:
    needs = job.get("needs") or []
    return [str(needs)] if isinstance(needs, str) else [str(n) for n in needs]


def _behind_parked(name: str, jobs: dict, seen: frozenset = frozenset()) -> bool:
    """Does this job's `needs` chain pass through the parked job?"""
    if name == PARKED_JOB:
        return True
    job = jobs.get(name)
    if not isinstance(job, dict) or name in seen:
        return False
    return any(_behind_parked(n, jobs, seen | {name}) for n in _needs(job))


def _actions_commands(conf) -> tuple[list[str], list[str]]:
    """The `run:` lines of every step of every job in a workflow, split into
    (can stop a merge, cannot).

    Only `run:` counts, for the same reason as on GitLab: every step carries a
    `name:` that mentions its script, so searching the whole YAML for a filename
    approves a job that is nothing but a heading.

    The split is the finding from #1633: a guard called only from a job in a
    dispatch-only workflow, or from a job parked behind `needs: parked`, was
    counted as "has a job" though nothing it finds can reach a pull request.
    That is the mirror's shape again -- a check by that name exists, and it
    stops nothing -- so those lines are handed back separately and judged with
    the mirror-only guards rather than with the blocking ones.
    """
    blocking: list[str] = []
    parked: list[str] = []
    workflow_blocks = _can_block(conf)
    jobs = (conf or {}).get("jobs") or {}
    for name, job in jobs.items():
        if not isinstance(job, dict):
            continue
        target = blocking if workflow_blocks and not _behind_parked(name, jobs) else parked
        for step in job.get("steps") or []:
            if isinstance(step, dict) and isinstance(step.get("run"), str):
                target.append(step["run"])
    return blocking, parked


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
    parked: list[str] = []
    workflows = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    for wf in workflows:
        try:
            blocking, unblockable = _actions_commands(
                yaml.safe_load(wf.read_text(errors="replace"))
            )
            actions.extend(blocking)
            parked.extend(unblockable)
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
    gh_parked = "\n".join(parked)
    gl = "\n".join(gitlab)
    ci = gh + "\n" + gh_parked + "\n" + gl

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

    # A guard that runs only on the mirror stops nothing. Neither does one whose
    # only GitHub job is in a dispatch-only workflow or parked behind a job that
    # always fails: the name is in a `run:`, and no pull request ever sees the
    # result. Both shapes are judged here as one list -- "stands before no
    # merge" -- against the same register and the same ratchet, because the
    # question a reader has to answer is the same: why is this not in front of
    # the merge, and where is that written down?
    mirror_only = [
        p.name for p in scripts
        if p.name not in ALLOWED and not _runs(p.name, gh)
        and (_runs(p.name, gl) or _runs(p.name, gh_parked))
    ]
    shape = {
        n: ("only on GitLab" if _runs(n, gl) and not _runs(n, gh_parked)
            else "only from a dispatch-only or parked job" if not _runs(n, gl)
            else "on GitLab and from a dispatch-only or parked job")
        for n in mirror_only
    }
    print(
        f"[guard-jobs] {len(scripts)} script(s), {len(ALLOWED)} deliberately by "
        f"hand, {len(mirror_only)} standing before no merge "
        f"({sum(1 for v in shape.values() if v == 'only on GitLab')} only on GitLab, "
        f"{sum(1 for v in shape.values() if v != 'only on GitLab')} only from a "
        "dispatch-only or parked job)"
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
            "\nThese guards stand before no merge -- only on the mirror, or only in\n"
            "a job that fires on workflow_dispatch or waits behind `needs: parked`\n"
            "-- so a failure stops nothing, and no reason for that is written down:\n",
            file=sys.stderr,
        )
        for name in sorted(undeclared):
            print(f"  scripts/ci/{name}  ({shape[name]})", file=sys.stderr)
        print(
            f"\nEither give it a job in .github/workflows/ that runs on pull_request\n"
            f"or push, or add it to {REGISTER.relative_to(REPO)} with reason and why.",
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
                print(f"  {name}: now runs in GitHub Actions before a merge -- remove the entry",
                      file=sys.stderr)
            else:
                print(f"  {name}: runs on neither pipeline", file=sys.stderr)
        print(
            "\nA reason that stays behind reads as current. Remove it in the same\n"
            "change that made it untrue.",
            file=sys.stderr,
        )

    if mirror_only:
        print(
            "[guard-jobs] standing before no merge, so stopping nothing:",
            file=sys.stderr,
        )
        for name in sorted(mirror_only):
            reason = register.get(name, {}).get("reason", "NO REASON WRITTEN DOWN")
            print(f"[guard-jobs]   scripts/ci/{name}  ({reason}; {shape[name]})",
                  file=sys.stderr)

    if len(mirror_only) != SPIEGEL_RATEL:  # RATCHET
        problems += 1
        direction = "more" if len(mirror_only) > SPIEGEL_RATEL else "fewer"
        print(
            f"\n[guard-jobs] {len(mirror_only)} guards stand before no merge, "
            f"{direction} than the {SPIEGEL_RATEL} written down here.",
            file=sys.stderr,
        )
        if len(mirror_only) > SPIEGEL_RATEL:
            print(
                "One has been added. Put a job on it in .github/workflows/ that runs\n"
                "on pull_request or push, or raise SPIEGEL_RATEL and add the entry to\n"
                f"{REGISTER.relative_to(REPO)} saying why this guard stays out of\n"
                "the way of a merge.",
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
        f"[guard-jobs] every guard has a job; the {len(mirror_only)} standing before "
        "no merge each carry a written reason"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
