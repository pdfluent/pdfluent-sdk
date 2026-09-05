#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A self-hosted label nobody answers is a queue entry pretending to be a gate.

Four workflows asked for `[self-hosted, xfa-corpus]`. That runner does not
exist: this repository has one self-hosted runner, labelled `xfa-fast`. So every
automatic run of those four sat in the queue until GitHub abandoned it roughly a
day later -- holding a slot, keeping the pipeline permanently busy, and testing
nothing (#276).

It never showed as a failure. A queued job is not red; it simply waits, and a
gate that waits forever looks exactly like a gate that has not got round to you
yet.

Labels that resolve to a machine we rent on demand are exempt: an ephemeral
runner's label is created at the moment the instance registers, so it is
correctly absent when nothing is running.

# NO-FLOOR: this compares two lists that both come from live sources. It cannot
# quietly find fewer -- an unreachable source is announced instead.
"""

from __future__ import annotations

import os, json
import pathlib
import re
import subprocess
import sys

import yaml

FLOWS = pathlib.Path(".github/workflows")
# Created when an instance registers itself, so absent by design between runs.
EFEMEER = re.compile(r"needs\.|matrix\.|^hetzner$|^gh-runner-")


def geregistreerd() -> set[str] | None:
    # `gh` is not installed on the desktop runner, and OSError from subprocess is
    # not a return code -- an uncaught one exits before a single line is printed,
    # which is a crash pretending to be a failed check. Missing tooling has to
    # announce itself.
    try:
        r = subprocess.run(
            ["gh", "api", "repos/{owner}/{repo}/actions/runners", "--jq",
             "[.runners[] | select(.status==\"online\") | .labels[].name] | unique"],
            capture_output=True, text=True, check=False, timeout=60,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if r.returncode != 0:
        return None
    try:
        return set(json.loads(r.stdout))
    except json.JSONDecodeError:
        return None


# Labels inside a per-event expression, e.g.
#   ${{ github.event_name == 'push' && fromJSON('["self-hosted","xfa-fast"]')
#       || 'ubuntu-latest' }}
# Both branches count: the job runs on one of them depending on the event, and a
# label that answers on neither is the thing this guard is for.
# The array is single-quoted and contains double quotes -- ["self-hosted",…] --
# so the content class cannot exclude quotes, which is what made the first
# version return only the other branch.
# re.DOTALL: YAML block scalars wrap an expression across lines, and `.` does
# not cross a newline without it -- so a multi-line fromJSON returned only the
# other branch and the self-hosted labels vanished. Same failure as the quoting
# bug one commit earlier, reached by line breaks instead. (T1 review, #1648)
_JSON_LIJST = re.compile(r"""fromJSON\(\s*'(\[.*?\])'\s*\)""", re.DOTALL)
_LOSSE_STRING = re.compile(r"""(?<!\.)'([A-Za-z0-9][A-Za-z0-9._-]*)'""")


# Runner images GitHub hosts. `-latest` and the pinned forms.
GEHOST = re.compile(r"^(ubuntu|windows|macos)-(latest|\d[\w.-]*)$")


def _weigert(step) -> bool:
    """A step that cannot succeed: a `run:` whose last command is a non-zero exit.

    An `if:` on the step disqualifies it -- a condition that is false makes the
    step a no-op and the job green, which is a queue entry with extra steps.

    Restored from 715e4121, which the #1543 reconciliation dropped (#319).
    """
    if not isinstance(step, dict) or "if" in step:
        return False
    script = step.get("run")
    if not isinstance(script, str):
        return False
    for line in reversed(script.strip().splitlines()):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = re.fullmatch(r"exit\s+([0-9]+)", line)
        return bool(m) and m.group(1) != "0"
    return False


def geparkeerd(doc, job) -> bool:
    """Is this job queued behind a job that cannot pass?

    Not "does it say geparkeerd in a comment" -- a comment does not stop a run.

    Two escapes are honoured. A blocker with `continue-on-error: true` does not
    actually block, and a blocker carrying an `if:` is a switch somebody can
    flip without touching this file; neither makes the dependant unstartable.
    """
    needs = job.get("needs")
    if isinstance(needs, str):
        needs = [needs]
    if not isinstance(needs, list):
        return False
    jobs = doc.get("jobs") or {}
    for name in needs:
        blocker = jobs.get(name)
        if not isinstance(blocker, dict):
            continue
        if blocker.get("continue-on-error") is True or "if" in blocker:
            continue
        if any(_weigert(s) for s in blocker.get("steps") or []):
            return True
    return False


def gevraagd(job) -> list[str]:
    ro = job.get("runs-on")
    if isinstance(ro, list):
        return [x for x in ro if isinstance(x, str)]
    if not isinstance(ro, str):
        return []
    if "${{" not in ro:
        return [ro]
    # An expression used to yield NO labels at all, so every job written this way
    # was invisible here: rename xfa-fast and each one stays green while the rest
    # of the file goes red. #311 puts nine jobs into this shape, which would have
    # turned one blind spot into nine. (T3 review, #1648)
    # A fromJSON whose argument is not a literal -- fromJSON(env.RUNNERS) --
    # cannot be read here. Returning [] made it indistinguishable from "nothing
    # to check", which is the pass this guard exists to refuse. (T1 review, #1648)
    if "fromJSON(" in ro and not _JSON_LIJST.search(ro):
        return ["__onleesbaar__"]
    uit: list[str] = []
    rest = ro
    for m in _JSON_LIJST.finditer(ro):
        try:
            uit.extend(x for x in json.loads(m.group(1)) if isinstance(x, str))
        except json.JSONDecodeError:
            pass
        rest = rest.replace(m.group(0), " ")
    # The other branch is a bare quoted runner name; the comparison operands
    # ('push', 'pull_request') are not, so anything containing a dot or matching
    # an event name is dropped.
    EVENTS = {"push", "pull_request", "schedule", "workflow_dispatch",
              "workflow_call", "release", "merge_group"}
    for m in _LOSSE_STRING.finditer(rest):
        naam = m.group(1)
        if naam not in EVENTS and not naam.startswith("github"):
            uit.append(naam)
    return uit


# THE DECISION, 05-09-2026 (#276). These four are manual and stay manual.
#
# The issue offered two ways out and asked for one answer. Both were measured
# before this was written down:
#
#   1. Register `xfa-corpus` on the desktop that already answers `xfa-fast`.
#      A runner is not a corpus. All 500 entries in CI_CORPUS_MANIFEST.json
#      name an absolute path on the external disk that now enumerates and
#      refuses every read (#264), and 498 of those 500 filenames have never
#      existed in this repository's history -- so the label would resolve and
#      the four would then fall into their own mini-mode, comparing two in-repo
#      fixtures against a pass rate calibrated on 500 documents. A gate that
#      measures two files and reports on five hundred is worse than one that
#      says it cannot run.
#   2. Ship the corpus to the ephemeral instance. There is nothing to ship,
#      for the same reason: no machine this repository can reach holds those
#      500 files, and the manifest records no origin to fetch them from.
#
# So neither is executable, and "parked until a runner exists" was never the
# right sentence -- the runner was never the missing part. What is missing is a
# corpus with a recorded origin, the way corpus/GATE_CORPUS_MANIFEST.json
# records one: a named suite, a commit, a path inside it and a SHA-256, which is
# why ci-ephemeral can fetch its 500 on every pull request while these four
# cannot fetch a single one of theirs.
#
# Each row therefore carries its reason, and the reason carries the condition
# that removes the row. The condition is not left to a reader: it is measured
# against the tree on every run by `de_corpusreden_geldt_nog` below.
_WAAROM_CORPUS = (
    "manual by decision (#276): its corpus is 500 entries in "
    "corpus/CI_CORPUS_MANIFEST.json that name a path on a dead disk and no "
    "origin to fetch from, so neither a runner on the corpus machine nor the "
    "corpus on a runner is available. Un-park this row when that manifest "
    "records where its documents come from, the way GATE_CORPUS_MANIFEST.json "
    "does."
)

REGISTER = {
    ("bench.yml", "benchmark", "xfa-corpus"): _WAAROM_CORPUS,
    ("crash-guard.yml", "crash-guard", "xfa-corpus"): _WAAROM_CORPUS,
    ("gate-ci.yml", "gate", "xfa-corpus"): _WAAROM_CORPUS,
    ("wasm-gate.yml", "wasm-gate", "xfa-corpus"): _WAAROM_CORPUS,
}

CORPUS_MANIFEST = pathlib.Path("corpus/CI_CORPUS_MANIFEST.json")
# What a recorded origin looks like: something a second machine could act on.
# A path beginning with `/` is a fact about one host and nobody else.
_TE_HALEN = re.compile(r"^(https?|git|ssh)://", re.I)


def de_corpusreden_geldt_nog(pad: pathlib.Path | None = None):
    """Is the reason those four rows give still true of the tree?

    A register whose rows explain themselves is still a register somebody has to
    re-read. This one states a fact about a file -- "that manifest records no
    origin" -- and a fact about a file can be checked, so it is, every run.

    Returns (True, detail) while every entry names only a machine path;
    (False, detail) as soon as one records an origin another machine could use,
    which is the day these gates can come back; and (None, detail) when the
    manifest cannot be read at all, because "I could not check" is not "still
    true" and must not be reported as one.
    """
    pad = pad or CORPUS_MANIFEST
    if not pad.is_file():
        return None, f"{pad} is missing, so the register's reason cannot be checked"
    try:
        doc = json.loads(pad.read_text())
    except (json.JSONDecodeError, OSError) as fout:
        return None, f"{pad} cannot be read: {fout}"
    entries = doc.get("entries") if isinstance(doc, dict) else None
    if not isinstance(entries, list) or not entries:
        return None, f"{pad} lists no entries, so there is nothing to judge"
    machinepad, herkomst = 0, []
    for e in entries:
        if not isinstance(e, dict):
            continue
        bron = str(e.get("source") or "")
        if _TE_HALEN.match(bron):
            herkomst.append(bron)
        elif bron.startswith("/"):
            machinepad += 1
        elif bron and (pad.parent.parent / bron).is_file():
            # A repo-relative source that resolves is the corpus arriving in the
            # tree, which is the other way this reason can expire.
            herkomst.append(bron)
    if herkomst:
        return False, (f"{len(herkomst)} of {len(entries)} entries now record an "
                       f"origin (first: {herkomst[0]}); the corpus is reachable")
    return True, (f"{machinepad} of {len(entries)} entries still name only a path "
                  "on a machine")


def main() -> int:
    if not FLOWS.is_dir():
        return 1

    # The register first, and deliberately before the runner list. Whether the
    # exemption is still earned is a question about files in this tree, so it
    # can be answered in GitHub Actions -- where the label comparison below
    # cannot run at all, because /actions/runners needs a scope GITHUB_TOKEN is
    # not allowed to have. Putting this after the early return would have made
    # the one half that CI can check the half CI never reaches.
    zonder_reden = sorted(k for k, v in REGISTER.items() if not str(v or "").strip())
    if zonder_reden:
        print("[labels] FATAL: a row in the register carries no reason, which makes "
              "it a name on a list again:", file=sys.stderr)
        for w, j, l in zonder_reden:
            print(f"  {w} :: {j} wants `{l}` and says nothing about why that is "
                  "acceptable", file=sys.stderr)
        return 1

    geldt, hoezo = de_corpusreden_geldt_nog()
    if geldt is None:
        print(f"[labels] FATAL: {hoezo}. The four rows below are excused on a claim "
              "about that file; unable to check it, this cannot report a pass.",
              file=sys.stderr)
        return 1
    if not geldt:
        print("[labels] FATAL: the register's reason has expired -- " + hoezo + ".",
              file=sys.stderr)
        for w, j, l in sorted(REGISTER):
            print(f"  {w} :: {j} is parked because that corpus could not be "
                  f"reached. It can now. Un-park it, or rewrite its reason.",
                  file=sys.stderr)
        return 1
    print(f"[labels] register: {len(REGISTER)} row(s), reason holds -- {hoezo}.")

    online = geregistreerd()
    if online is None:
        print("SKIPPED (not a pass): could not read the runner list, so no label was "
              "checked against anything.", file=sys.stderr)
        # Say where this can and cannot work, because the answer is structural
        # and somebody will otherwise try to fix it with a token. Measured
        # 01-09-2026: `administration` is not a settable scope for
        # GITHUB_TOKEN -- actionlint enumerates every available scope and it is
        # not among them -- and /actions/runners requires it. So no workflow
        # token can read this list, whatever else is wired up. It needs `gh`
        # with a keyring, which is the local gate, or a PAT in a secret, which
        # is a decision nobody has taken (#290).
        print("  This works from scripts/ci/local_ci_gate.sh, where `gh` is "
              "authenticated. It cannot work in GitHub Actions: /actions/runners "
              "needs `administration: read`, which GITHUB_TOKEN cannot be granted.",
              file=sys.stderr)
        # A skip in CI is a pass in CI, and CLAUDE.md's Definition of Done does not
        # allow that. Locally this is honest -- there is a `gh` to ask, and a
        # missing one is the operator's problem. In Actions there is no `gh` and
        # GITHUB_TOKEN cannot be granted `administration: read`, so the skip is
        # permanent: the step would report success on every run without checking
        # a single label. (codex, #1639)
        if os.environ.get("CI"):
            print("  Running in CI, where this can never succeed. Reporting a "
                  "pass here would be a green tick for a check that did not "
                  "happen.", file=sys.stderr)
            return 1
        return 0

    ontbreekt = []
    for pad in sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml")):
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError:
            continue
        if not isinstance(doc, dict):
            continue
        on = doc.get(True) or doc.get("on") or {}
        # `on:` has three legal shapes: a mapping, a sequence, or a bare string.
        # Wrapping a sequence in a list left an unhashable list inside `namen`,
        # so any workflow using `on: [push, workflow_dispatch]` crashed this
        # guard instead of being checked by it.
        if isinstance(on, dict):
            namen = list(on)
        elif isinstance(on, (list, tuple)):
            namen = list(on)
        else:
            namen = [on]
        # Every trigger, not only the automatic ones. Three runs sat queued for
        # nine hours on a label no runner carried (#281) and this guard reported
        # OK, because their workflows are dispatch-only and fell outside exactly
        # the check written to catch them. A job that can never be assigned is
        # stuck whoever started it.
        vanzelf = namen or ["(no trigger)"]
        for naam, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            labels = gevraagd(job)
            if "self-hosted" not in labels:
                continue
            for label in labels:
                if label == "self-hosted" or EFEMEER.search(label):
                    continue
                # GitHub provides these; no runner of ours registers them. They
                # turn up here because a per-event `runs-on` names BOTH branches
                # -- the job runs on one OR the other depending on the event, not
                # on all of them at once -- and reading the alternatives as a
                # single label set made the hosted branch look like a
                # self-hosted label nobody answers. (T3 review, #1648)
                if label == "__onleesbaar__":
                    ontbreekt.append((pad.name, naam,
                                      "an unreadable fromJSON() argument", vanzelf,
                                      geparkeerd(doc, job)))
                    continue
                if GEHOST.match(label):
                    continue
                if label not in online:
                    ontbreekt.append((pad.name, naam, label, vanzelf,
                                      geparkeerd(doc, job)))

    print(f"[labels] online labels: {', '.join(sorted(online)) or 'none'}")
    if not ontbreekt:
        print("[labels] OK: every self-hosted label any job asks for has a runner.")
        return 0

    # The four corpus gates, and WHY each is manual rather than merely listed.
    # Named individually so the register cannot quietly grow, and checked in
    # both directions so it cannot quietly shrink either -- a baseline that only
    # fails upward stops being a baseline. The reasons are enforced twice over:
    # a row without one is refused, and the fact they all rest on is measured
    # against the tree by `de_corpusreden_geldt_nog` on every run.
    BEKEND = set(REGISTER)
    # The exemption is for jobs nobody can start by accident. If one of these
    # workflows re-enables push or schedule it queues on every commit, which is
    # the failure this guard exists for -- so the trigger list is part of what
    # is excused, not something the baseline may drop.
    HANDMATIG = {"workflow_dispatch", "workflow_call", "repository_dispatch"}
    handmatig_stuk = {(w, j, l) for w, j, l, t, _ in ontbreekt if set(t or []) <= HANDMATIG}
    automatisch = [r for r in ontbreekt if not set(r[3] or []) <= HANDMATIG]
    # DERIVED, not listed. `BEKEND` says which jobs may wait; the needs-graph
    # says which ones actually cannot start. The two must agree, or the list
    # is a hand-kept answer to a question the tree already answers (#319).
    geparkeerd_stuk = {(w, j, l) for w, j, l, _, gp in ontbreekt if gp}

    def uitleg(rijen) -> None:
        for workflow, job, label, triggers, *_ in rijen:
            print(f"  {workflow} :: {job} wants `{label}` on {triggers}", file=sys.stderr)
        print(
            "\nThose runs queue until GitHub abandons them about a day later. A queued "
            "job is not red -- it waits, and a gate that waits forever looks like one "
            "that has not got round to you yet. Register the runner, point the job at "
            "one that exists, or make the workflow dispatch-only until it can run. "
            "(#276)",
            file=sys.stderr,
        )

    if automatisch:
        print(file=sys.stderr)
        print(f"[labels] FATAL: {len(automatisch)} job(s) ask for a missing label on an "
              "automatic trigger; no baseline covers those.", file=sys.stderr)
        uitleg(automatisch)
        return 1

    nieuw = handmatig_stuk - BEKEND
    if nieuw:
        print(file=sys.stderr)
        print(f"[labels] FATAL: {len(nieuw)} job(s) ask for a label no runner answers:",
              file=sys.stderr)
        uitleg([r for r in ontbreekt if (r[0], r[1], r[2]) in nieuw])
        return 1

    opgelost = BEKEND - handmatig_stuk
    if opgelost:
        print(f"FAIL: {len(opgelost)} known-stuck job(s) can now be assigned: "
              f"{sorted(opgelost)}. Remove them from BEKEND so the next one is caught.",
              file=sys.stderr)
        return 1

    # The graph against the list. `715e4121` answered this structurally and the
    # #1543 reconciliation dropped it, leaving the distinction between "geparkeerd
    # on purpose" and "blocked because something ahead of it broke" resting on a
    # list somebody maintains (#319).
    #
    # Measured on 04-09-2026: the four in BEKEND are exactly the four the graph
    # derives. The list is right; nothing was keeping it right.
    namen_bekend = {(w, j) for w, j, _ in BEKEND}
    namen_graaf = {(w, j) for w, j, _ in geparkeerd_stuk}
    niet_geparkeerd = namen_bekend - namen_graaf
    niet_gelijst = namen_graaf - namen_bekend
    if niet_geparkeerd or niet_gelijst:
        print(file=sys.stderr)
        print("[labels] FATAL: the baseline and the needs-graph disagree about which "
              "jobs cannot start.", file=sys.stderr)
        for w, j in sorted(niet_geparkeerd):
            print(f"  {w} :: {j} is excused by BEKEND but nothing ahead of it refuses. "
                  "It is waiting for a runner, not geparkeerd.", file=sys.stderr)
        for w, j in sorted(niet_gelijst):
            print(f"  {w} :: {j} is queued behind a job that cannot pass, and is not in "
                  "BEKEND. Add it, or unblock it.", file=sys.stderr)
        return 1

    print(f"[labels] OK: {len(BEKEND)} job(s) are manual by decision (#276), each "
          f"with its reason, and the needs-graph derives the same {len(namen_graaf)}; "
          "no new label is unanswered.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
