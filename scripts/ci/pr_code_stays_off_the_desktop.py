#!/usr/bin/env python3
"""A pull request must not run on the persistent runner (#311).

ci.yml states the boundary in its own header:

    xfa-fast is a persistent desktop runner, not an isolated ephemeral one, so
    a PR's Cargo build scripts/proc-macros would execute as the runner user with
    host access if PR runs used it. Only code that's already merged (push)
    touches it. This repo is private today but is slated to go public at LC10 --
    this boundary needs to hold before that, not be added after.

It was documented and not enforced, so a workflow added on 02-09-2026 broke it
on its first day -- by copying `runs-on` from a job that was already breaking it.
That is how an unenforced rule spreads: the existing code is the documentation
people actually read.

A job reachable from `pull_request` and landing on a self-hosted label runs
PR-AUTHORED files as the runner user. It does not need to be a Cargo build; every
guard job here runs `python3 scripts/ci/<something>.py` out of the checkout, and
on a pull request those scripts are whatever the PR says they are.

The rule: if a workflow can be triggered by `pull_request`, none of its jobs may
request a self-hosted runner unconditionally. Choosing per event is fine and is
what the fixed workflow does:

    runs-on: ${{ github.event_name == 'pull_request' && 'ubuntu-latest'
                 || fromJSON('["self-hosted","xfa-fast"]') }}
"""
from __future__ import annotations
import importlib.util, pathlib, re, sys, yaml

# ONE recogniser for the canonical runs-on expression, shared with
# orchestration_stays_hosted.py rather than written twice. Two guards reading the
# same construct with two patterns is how #1648 happened: my expression said the
# same thing the other way round, and that guard -- correctly -- did not know it.
_spec = importlib.util.spec_from_file_location(
    "orchestration_stays_hosted",
    pathlib.Path(__file__).resolve().parent / "orchestration_stays_hosted.py")
_osh = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_osh)
ALLEEN_BIJ_PUSH = _osh.ALLEEN_BIJ_PUSH
PER_GEBEURTENIS_VEILIG = _osh.per_gebeurtenis_veilig
DESKTOP_TOEGESTAAN = _osh.desktop_toegestaan

REPO = pathlib.Path(__file__).resolve().parents[2]
FLOW = REPO / ".github" / "workflows"

# Each entry is a job that predates the guard, with the reason it is still here.
# They are not exempt because they are safe -- they are the backlog this guard
# was written to close, recorded so the count cannot quietly grow. (#311)
# Jobs whose runner comes from another job's output. Recorded rather than
# resolved: the guard cannot see what machine appears, so the reason has to be
# written and the supplier named. If the supplier changes, the claim was made
# about something else and this goes red.
DYNAMIC: dict[str, dict] = {
    "ci-ephemeral.yml:workspace": {
        # DORMANT since 02-09-2026: this branch removes ci-ephemeral.yml's
        # pull_request trigger, so the job is no longer reachable from a pull
        # request and this entry matches nothing today.
        #
        # Kept rather than deleted, and that is the opposite of what the KNOWN
        # sweep below demands of a register -- deliberately. KNOWN records
        # exceptions that must SHRINK; this records a claim someone has to read
        # BEFORE they touch create-runner, and deleting it would take the
        # warning away exactly when the trigger comes back.
        #
        # Guarded rather than trusted: a dormant entry whose job becomes
        # pull_request-reachable again while still marked dormant is a failure,
        # because the claim was written about a job nobody was running.
        # (T1 review, #1649)
        "dormant": True,
        "producer": "create-runner",
        "why": (
            "the one job that SHOULD run the pull request's own code: "
            "create-runner brings up a throwaway instance for exactly that, and "
            "its checkout is deliberately unpinned. What nothing guards is that "
            "create-runner keeps supplying an ephemeral machine rather than "
            "falling back to the desktop -- named here so the assumption is at "
            "least visible (#311)"
        ),
    },
}

KNOWN: dict[str, str] = {
    # Empty, and it has to stay that way. When this guard was written it held
    # nine jobs -- five in ci.yml, two in security-audit.yml, two in
    # ci-ephemeral.yml -- recorded so the count could not quietly grow. #311
    # moved all nine, so the register describes nothing and is gone.
    #
    # An entry that stops matching anything is itself a failure here, which is
    # what forced this to be emptied in the same change rather than left as a
    # list nobody revisits.
}

# Runner images GitHub hosts. Anything else -- including a bare custom label
# like `xfa-fast`, with no "self-hosted" in it -- is a request for one of our
# own machines. Testing for the WORD "self-hosted" let exactly that through.
# (T1 review, #1635)
GEHOST = re.compile(r"^(ubuntu|windows|macos)-(latest|\d[\w.-]*)$")

MINIMUM_WORKFLOWS = 10  # FLOOR


def _judge(key: str, job: dict, fname: str, jname: str, runs_on=None,
           target_only: bool = False, events: set[str] | None = None,
           triggers=None) -> list[str]:
    """Judge ONE runner choice for one job.

    Split out because a job can have several: a matrix supplies a list, and a
    reusable workflow moves them into another file. The old code judged
    `job["runs-on"]` once and skipped anything that did not literally contain
    "self-hosted" -- which is three different spellings of the same request.
    (T1 review, #1635)
    """
    if runs_on is None:
        runs_on = job.get("runs-on")
    labels = runs_on if isinstance(runs_on, list) else [runs_on]
    text = str(runs_on)
    # An absent, empty, or all-blank `runs-on` names NO runner, and `all()` over
    # nothing is True -- so those three shapes reported "every label is a hosted
    # image" and passed on the strength of having said nothing. Vacuous green,
    # the same shape as "empty is not a verdict" one function over. Which runner
    # a job takes cannot be read off a field that is not there. (codex, #1649)
    echte = [l for l in labels if isinstance(l, str) and l.strip()]
    if not echte:
        return [f"{key} has no readable `runs-on` ({runs_on!r}). Which runner it "
                "takes cannot be established, and an empty field is not a hosted "
                "runner -- it is an unanswered question."]
    # NOT `"self-hosted" in text`: a bare custom label like `xfa-fast` is a
    # self-hosted request without the word in it, and that skip was the hole.
    hosted = all(GEHOST.match(l) for l in echte)
    if hosted:
        return []
    # A LIST demands EVERY label. `["${{ ...event_name == 'push'... }}",
    # xfa-fast]` asks for xfa-fast on a pull request too -- the expression only
    # decides what the OTHER element resolves to -- so a per-event condition
    # sitting beside a literal desktop label excuses nothing. The check read
    # `str(runs_on)`, found the condition anywhere in the repr of the whole
    # list, and approved its neighbours with it.
    #
    # I looked for this shape after the first report and said it did not
    # reproduce. It does; I built the fixture with a `fromJSON` expression,
    # which GEHOST rejects, so the list was flagged for the wrong reason and I
    # read that as the guard working. The leaking form is a BARE-STRING
    # expression, which T1's fixture used. (T1 review, #1649)
    letterlijk = [l for l in echte if "${{" not in l]
    onvoorwaardelijk = [l for l in letterlijk if not GEHOST.match(l)]
    expressies = [l for l in echte if "${{" in l]
    if not (isinstance(runs_on, list) and onvoorwaardelijk):
        # Judge the ELEMENTS, not the list's repr. Handing `str(["${{ ... }}"])`
        # to the predicate asks it about brackets and escaped quotes, and it
        # answered "not safe" about a runner choice that is.
        kandidaten = expressies if expressies else [text]
        # The caller's TRIGGER BLOCK, not only its event names. `push` is
        # merged code when its branch filter says so, and passing the names
        # alone let an unfiltered `push` count as safe -- including down the
        # reusable-workflow path, where the inner job's canonical
        # `event_name == 'push'` expression was then approved. Same finding as
        # the branch-filter one, reached through the other door. (codex, #1649)
        toegestaan = DESKTOP_TOEGESTAAN(triggers)
        if all(PER_GEBEURTENIS_VEILIG(k, events, toegestaan) for k in kandidaten):
            return []
    # A runner supplied by another job. This is the one shape where running the
    # pull request's own code is CORRECT -- ci-ephemeral's `workspace` is meant
    # to, on a throwaway machine. The guard cannot resolve the label, so it
    # cannot confirm the machine is throwaway; it requires the claim to be
    # written down instead, naming the job that must keep supplying it.
    # (T1 review, #1635)
    if "needs." in text and ".outputs." in text:
        producer = text.split("needs.", 1)[1].split(".", 1)[0]
        entry = DYNAMIC.get(key)
        if entry is None:
            return [f"{key} takes its runner from `{producer}`, so this guard "
                    "cannot tell whether that machine is ephemeral. Record it in "
                    "DYNAMIC with the job that supplies it and why running "
                    "pull-request code there is safe."]
        if entry.get("producer") != producer:
            return [f"{key} now takes its runner from `{producer}`, while "
                    f"DYNAMIC records `{entry.get('producer')}`. The claim was "
                    "made about a different supplier."]
        return []
    checkouts = [st for st in (job.get("steps") or [])
                 if "actions/checkout" in str(st.get("uses", ""))]
    if target_only:
        # Under pull_request_target the safety is INVERTED: a checkout with no
        # ref takes the base, which is the safe case, and naming the head is
        # what puts the pull request's code on the machine. So "every checkout
        # pinned to base.sha" is the wrong test here -- absence of a head ref is.
        # Enumerating the unsafe spellings does not converge. head.sha, then
        # head.ref, then refs/pull/N/merge, then github.head_ref -- four rounds,
        # each one "the last one", and the fourth is the SHORTEST and the one
        # written most often from memory. The list was the wrong shape: it asked
        # which names mean the head, and there is no closed answer to that.
        #
        # Inverted, it is closed. Under pull_request_target a checkout is safe
        # when it takes the BASE, and there are only two ways to be sure of that:
        # no ref at all (the event's default), or a ref that says base. Anything
        # else -- including an expression this guard has never seen -- is refused
        # and the message says how to declare it. Fail closed, so the next
        # spelling costs a message rather than a breach. (T1 review, #1635)
        def takes_the_base(step: dict) -> bool:
            with_ = step.get("with") or {}
            if "ref" not in with_:
                return True          # the event's default IS the base
            return "base." in str(with_["ref"])

        if all(takes_the_base(st) for st in checkouts):
            return []
        return [f"{key} runs on {runs_on} under pull_request_target and checks "
                "out a ref that is not the base. That trigger runs with the "
                "repository's secrets, so PR-authored code there is the "
                "pull_request case with credentials attached. If the ref IS the "
                "base, say so -- `base.sha` -- or drop the ref and take the "
                "event's default."]
    if checkouts and all(
            "pull_request.base.sha" in str((st.get("with") or {}).get("ref", ""))
            for st in checkouts):
        return []
    if key in KNOWN:
        return []
    return [f"{key} runs on {runs_on} and is reachable from pull_request. It "
            "executes PR-authored files as the runner user on the persistent "
            "desktop."]


def main() -> int:
    # GitHub reads .yaml as well. Globbing only .yml meant a workflow named
    # unsafe.yaml escaped this guard completely. (codex, #1635)
    files = sorted(list(FLOW.glob("*.yml")) + list(FLOW.glob("*.yaml")))
    if len(files) < MINIMUM_WORKFLOWS:  # FLOOR
        print(f"[pr-runner] FATAL: {len(files)} workflow(s) found, floor is "
              f"{MINIMUM_WORKFLOWS}. A scan that read almost nothing must not "
              "report a clean result.", file=sys.stderr)
        return 2

    problems: list[str] = []
    reachable: set[str] = set()
    stale: list[str] = []
    checked = 0
    seen: set[str] = set()
    for f in files:
        try:
            doc = yaml.safe_load(f.read_text()) or {}
        except yaml.YAMLError as exc:
            print(f"[pr-runner] FATAL: {f.name} does not parse: {exc}", file=sys.stderr)
            return 2
        # PyYAML reads a bare `on:` key as the boolean True.
        # `on:` has three legal shapes and PyYAML reads the bare key as True:
        #   on: pull_request            -> str
        #   on: [pull_request, push]    -> list
        #   on: {pull_request: {...}}   -> dict
        # Only the mapping was handled, so `on: [pull_request]` -- a perfectly
        # ordinary spelling -- skipped the whole workflow. (codex, #1635)
        on = doc.get("on", doc.get(True))
        if isinstance(on, str):
            events = {on}
        elif isinstance(on, list):
            events = {e for e in on if isinstance(e, str)}
        elif isinstance(on, dict):
            events = set(on)
        else:
            events = set()
        # `pull_request_target` is a pull-request trigger too, and a worse one:
        # it also hands the job the repository's secrets. Its DEFAULT is safe --
        # it runs the base ref, which is why the event exists -- but checking out
        # the head is the most common thing written under it, since that is how
        # you build a fork's code with a token. The rule ("no PR-authored code on
        # the desktop") always covered this; only the event list did not.
        # (T1 review, #1635)
        pr_events = events & {"pull_request", "pull_request_target"}
        if not pr_events:
            continue
        target_only = pr_events == {"pull_request_target"}
        for name, job in (doc.get("jobs") or {}).items():
            runs_on = job.get("runs-on")
            checked += 1
            # A reusable workflow puts the runner in ANOTHER file. `uses:` at job
            # level means the jobs that actually run live there, and this file
            # says nothing about them -- so the guard reported success over jobs
            # it had not seen. (T1 review, #1635)
            if job.get("uses"):
                target = str(job["uses"]).split("@")[0]
                if target.startswith("./"):
                    called = FLOW.parent.parent / target[2:]
                    if called.is_file():
                        inner = yaml.safe_load(called.read_text()) or {}
                        if not events:
                            problems.append(
                                f"{f.name}:{name} calls {target} and this guard "
                                "could not determine the caller's events. An "
                                "empty event set is not 'no risk'.")
                            continue
                        for iname, ijob in (inner.get("jobs") or {}).items():
                            # The CALLER's events, not the inner workflow's: a
                            # reusable workflow runs under whatever triggered the
                            # job that calls it, and passing nothing here let
                            # _judge treat a pull_request_target caller as
                            # eventless and approve it. (codex, #1649)
                            problems.extend(_judge(f"{called.name}:{iname}", ijob,
                                                   called.name, iname,
                                                   target_only=target_only,
                                                   events=events,
                                                   triggers=on))
                        continue
                problems.append(
                    f"{f.name}:{name} calls {job['uses']}, which this guard "
                    "cannot read. A job whose runner is defined elsewhere is not "
                    "a job that was checked.")
                continue
            # A matrix supplies the labels from `strategy.matrix`, so the
            # expression alone says nothing. Every value the matrix can produce
            # has to be judged, not the placeholder. (T1 review, #1635)
            candidates = [runs_on]
            if isinstance(runs_on, str) and "matrix." in runs_on:
                key = runs_on.split("matrix.", 1)[1].split("}")[0].strip()
                values = ((job.get("strategy") or {}).get("matrix") or {}).get(key)
                candidates = values if isinstance(values, list) else [runs_on]
                if not isinstance(values, list):
                    problems.append(
                        f"{f.name}:{name} chooses its runner from "
                        f"`matrix.{key}`, which this guard cannot resolve.")
                    continue
            key = f"{f.name}:{name}"
            reachable.add(key)
            if key in KNOWN:
                seen.add(key)
            for candidate in candidates:
                problems.extend(_judge(key, job, f.name, name, candidate,
                                       target_only, events, triggers=on))
            continue

    # A dormant DYNAMIC entry must stay unreachable. If its job turns up in the
    # pull_request-reachable set again, the claim is live and unexamined.
    for key, entry in sorted(DYNAMIC.items()):
        if entry.get("dormant") and key in reachable:
            stale.append(
                f"{key} is marked dormant in DYNAMIC, and is reachable from "
                "pull_request again. The claim was written about a job nobody "
                "was running; re-read it before the trigger goes back.")

    present = {f.name for f in files}
    for key in sorted(set(KNOWN) - seen):
        if key.split(":", 1)[0] not in present:
            continue
        stale.append(f"{key} is recorded as a known exception and no longer "
                     "matches anything. Remove the entry: a register that "
                     "outlives what it describes starts protecting nothing.")

    if checked == 0:
        print("[pr-runner] FATAL: no pull_request-triggered job was found at all.",
              file=sys.stderr)
        return 2

    if problems or stale:
        print(f"[pr-runner] FAIL:", file=sys.stderr)
        for o in problems + stale:
            print(f"    {o}", file=sys.stderr)
        # The remediation used to print the INVERSE expression -- the very form
        # finding 3 is about. Guidance that tells you to write what the guard
        # rejects is worse than no guidance: it is a wrong answer with the
        # authority of the tool. (codex, #1635)
        print("\n  Choose the runner per event, in the form both guards read:\n"
              "    runs-on: ${{ github.event_name == 'push'\n"
              "                 && fromJSON('[\"self-hosted\",\"xfa-fast\"]')\n"
              "                 || 'ubuntu-latest' }}\n"
              "\n  Or stay on the desktop and pin every checkout to the base"
              " revision:\n"
              "    ref: ${{ github.event_name == 'pull_request'\n"
              "             && github.event.pull_request.base.sha || github.sha }}",
              file=sys.stderr)
        return 1

    msg = (f"[pr-runner] OK: {checked} pull_request-reachable job(s); none runs "
           "pull-request code on the persistent desktop.")
    if KNOWN:
        msg += (f" {len(KNOWN)} recorded from before this guard, each still to "
                "be moved (#311).")
    print(msg)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
