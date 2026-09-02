#!/usr/bin/env python3
"""Two-way proof for pr_code_stays_off_the_desktop.py (#311).

Every case is a shape the first version let through. The guard recognised the
form in front of it rather than the class, and each of these is a legal spelling
of the same dangerous thing.
"""
from __future__ import annotations
import pathlib, shutil, subprocess, sys, tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import wegwerp_map  # noqa: E402

CI = pathlib.Path(__file__).resolve().parent
GUARD = CI / "pr_code_stays_off_the_desktop.py"

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


SAFE = ("${{ github.event_name == 'push' && fromJSON('[\"self-hosted\",\"xfa-fast\"]')"
        " || 'ubuntu-latest' }}")


def tree(workflows: dict[str, str]) -> pathlib.Path:
    td = wegwerp_map()
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / ".github" / "workflows").mkdir(parents=True)
    for name in (GUARD.name, "orchestration_stays_hosted.py"):
        shutil.copy(CI / name, root / "scripts" / "ci" / name)
    # The floor wants a population of workflows.
    for i in range(12):
        (root / ".github" / "workflows" / f"filler{i}.yml").write_text(
            "on:\n  push:\n    branches: [master]\njobs:\n  a:\n    runs-on: ubuntu-latest\n"
            "    steps:\n      - run: true\n")
    for fn, body in workflows.items():
        (root / ".github" / "workflows" / fn).write_text(body)
    return root


def run(root: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(root / "scripts" / "ci" / GUARD.name)],
                          capture_output=True, text=True)


def wf(on: str, runs_on: str, checkout_ref: str | None = None) -> str:
    step = "      - uses: actions/checkout@v4\n"
    if checkout_ref:
        step += f"        with:\n          ref: {checkout_ref}\n"
    return (f"{on}jobs:\n  guard:\n    runs-on: {runs_on}\n    steps:\n{step}"
            "      - run: python3 scripts/ci/thing.py\n")


MAP = "on:\n  pull_request:\n    branches: [master]\n"
LIST = "on: [pull_request, push]\n"
STR = "on: pull_request\n"
DESKTOP = '[self-hosted, xfa-fast]'
INVERSE = ("${{ github.event_name == 'pull_request' && "
           "fromJSON('[\"self-hosted\",\"xfa-fast\"]') || 'ubuntu-latest' }}")

print("pr code stays off the desktop — two-way")

r = run(tree({"ok.yml": wf(MAP, SAFE)}))
expect("the canonical per-event expression passes", r.returncode == 0,
       f"exit={r.returncode} {r.stderr[-160:]}")

r = run(tree({"bad.yml": wf(MAP, DESKTOP)}))
expect("an unconditional self-hosted job FAILS", r.returncode == 1, f"exit={r.returncode}")

# .yaml is a legal workflow extension; globbing only .yml missed it entirely.
r = run(tree({"unsafe.yaml": wf(MAP, DESKTOP)}))
expect("a .yaml workflow is scanned too", r.returncode == 1, f"exit={r.returncode}")
expect("  and it is named", "unsafe.yaml" in r.stderr, r.stderr[-160:])

# `on:` has three legal shapes; only the mapping was handled.
r = run(tree({"listform.yml": wf(LIST, DESKTOP)}))
expect("a list-form `on:` is reachable", r.returncode == 1, f"exit={r.returncode}")
r = run(tree({"strform.yml": wf(STR, DESKTOP)}))
expect("a string-form `on:` is reachable", r.returncode == 1, f"exit={r.returncode}")

# The inverse expression says the rule exactly backwards.
r = run(tree({"inverse.yml": wf(MAP, INVERSE)}))
expect("the INVERSE expression FAILS, it does not pass", r.returncode == 1,
       f"exit={r.returncode}")

# The second route: stay on the desktop, run merged code.
PIN = "${{ github.event_name == 'pull_request' && github.event.pull_request.base.sha || github.sha }}"
r = run(tree({"pinned.yml": wf(MAP, DESKTOP, PIN)}))
expect("a desktop job that pins the base revision passes", r.returncode == 0,
       f"exit={r.returncode} {r.stderr[-200:]}")

r = run(tree({"halfpinned.yml": wf(MAP, DESKTOP, PIN).replace(
    "      - run: python3 scripts/ci/thing.py\n",
    "      - uses: actions/checkout@v4\n      - run: python3 scripts/ci/thing.py\n")}))
expect("one unpinned checkout in the same job FAILS", r.returncode == 1,
       f"exit={r.returncode}")

# Three spellings of a self-hosted request that carry no "self-hosted" in the
# place the guard was looking. (T1 review, #1635)
r = run(tree({"bare.yml": wf(MAP, "xfa-fast")}))
expect("a bare custom label FAILS", r.returncode == 1, f"exit={r.returncode}")

r = run(tree({
    "outer.yml": f"{MAP}jobs:\n  call:\n    uses: ./.github/workflows/inner.yml\n",
    "inner.yml": ("on:\n  workflow_call:\njobs:\n  inner:\n"
                  "    runs-on: [self-hosted, xfa-fast]\n    steps:\n"
                  "      - uses: actions/checkout@v4\n      - run: true\n"),
}))
expect("a reusable workflow's inner job is judged", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and it is named", "inner.yml:inner" in r.stderr, r.stderr[-160:])

r = run(tree({"matrix.yml": (f"{MAP}jobs:\n  guard:\n    strategy:\n"
                            "      matrix:\n        runner: [[self-hosted, xfa-fast]]\n"
                            "    runs-on: ${{ matrix.runner }}\n    steps:\n"
                            "      - uses: actions/checkout@v4\n      - run: true\n")}))
expect("a matrix-supplied runner is resolved", r.returncode == 1,
       f"exit={r.returncode}")

# pull_request_target is a pull-request trigger and a worse one: it also carries
# the repository's secrets. Its safety is INVERTED -- no ref means the base ref,
# which is safe; naming the head is what puts PR code on the machine.
# (T1 review, #1635)
PRT_HEAD = ("on: pull_request_target\njobs:\n  build:\n"
            f"    runs-on: {DESKTOP}\n    steps:\n"
            "      - uses: actions/checkout@v4\n        with:\n"
            "          ref: ${{ github.event.pull_request.head.sha }}\n"
            "      - run: true\n")
PRT_BASE = ("on: pull_request_target\njobs:\n  build:\n"
            f"    runs-on: {DESKTOP}\n    steps:\n"
            "      - uses: actions/checkout@v4\n      - run: true\n")

r = run(tree({"prt.yml": PRT_HEAD}))
expect("pull_request_target checking out the HEAD FAILS", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and says the secrets make it worse", "secrets" in r.stderr,
       r.stderr[-160:])

# Four spellings of the head, and the fourth is the shortest -- github.head_ref
# uses an underscore, so a substring list built from the dotted names missed it.
# The test is inverted now (safe = takes the base), so an unseen spelling is
# refused rather than admitted; these stay as the record of what was tried.
for spelling in ("github.event.pull_request.head.ref",
                 "refs/pull/${{ github.event.number }}/merge",
                 "github.head_ref",
                 "some.field.nobody.has.written.yet"):
    body = PRT_HEAD.replace("github.event.pull_request.head.sha", spelling)
    r = run(tree({"prt.yml": body}))
    expect(f"pull_request_target with ref={spelling.split('.')[-1]} FAILS",
           r.returncode == 1, f"exit={r.returncode}")

r = run(tree({"prt.yml": PRT_HEAD.replace(
    "github.event.pull_request.head.sha",
    "github.event.pull_request.base.sha")}))
expect("pull_request_target naming the BASE explicitly passes",
       r.returncode == 0, f"exit={r.returncode} {r.stderr[-160:]}")

r = run(tree({"prt.yml": PRT_BASE}))
expect("pull_request_target without a ref passes (it takes the base)",
       r.returncode == 0, f"exit={r.returncode} {r.stderr[-160:]}")

r = run(tree({}))
expect("no pull_request job at all is FATAL, not a pass", r.returncode == 2,
       f"exit={r.returncode}")

# A LIST demands every label, so a per-event expression standing beside a
# literal desktop label excuses nothing -- the expression only decides what the
# OTHER element resolves to. T1's fixture, which I had looked for and wrongly
# reported as not reproducing: I built mine with a fromJSON expression, which
# is rejected for a different reason, and read that as the guard working.
LIJST_LEK = """
on: [pull_request]
jobs:
  j:
    runs-on: ["${{ github.event_name == 'push' && 'self-hosted' || 'ubuntu-latest' }}", xfa-fast]
    steps: [{run: echo}]
"""
r = run(tree({"lijst.yml": LIJST_LEK}))
expect("a desktop label beside a per-event expression FAILS", r.returncode == 1,
       f"exit={r.returncode} {r.stderr[-200:]}")
expect("  and the job is named", "lijst.yml" in r.stderr, r.stderr[-200:])

LIJST_LETTERLIJK = LIJST_LEK.replace(
    '"${{ github.event_name == \'push\' && \'self-hosted\' || \'ubuntu-latest\' }}", xfa-fast',
    "ubuntu-latest, xfa-fast")
r = run(tree({"lijst.yml": LIJST_LETTERLIJK}))
expect("  and so does the same list without the expression", r.returncode == 1,
       f"exit={r.returncode}")

# The legitimate single choice still passes: one element, one expression.
LIJST_OK = """
on: [pull_request]
jobs:
  j:
    runs-on: ["${{ github.event_name == 'push' && 'self-hosted' || 'ubuntu-latest' }}"]
    steps: [{run: echo}]
"""
r = run(tree({"lijst.yml": LIJST_OK}))
expect("a one-element list holding only the expression passes",
       r.returncode == 0, f"exit={r.returncode} {r.stderr[-200:]}")

  # The dormant-DYNAMIC rule is not exercised here: DYNAMIC names
# ci-ephemeral.yml:workspace specifically, and these fixtures build synthetic
# workflow trees that do not contain it. Proven against the real tree instead --
# putting the pull_request trigger back on ci-ephemeral.yml turns the guard red
# and names the entry. Said plainly rather than covered by an assertion that
# only greps the source for the word.
# The caller's BRANCH FILTER, not only its event names. `push` is merged code
# when its filter says so; an unfiltered push runs whatever is on any branch,
# which is the pull-request case with the review left out. Passing the names
# alone let that count as safe -- including down the reusable-workflow path,
# where the inner job's canonical expression was then approved.
#
# And the other half: an event the workflow does not trigger on cannot select
# anything, so it is UNREACHABLE, not unsafe. Refusing it flagged the canonical
# expression in a pull_request-only workflow, where the condition can never be
# true. A guard that cannot tell "this never happens" from "this is dangerous"
# spends its credibility on the first to protect against the second.
PUSH_LOS = "on:\n  pull_request:\n    branches: [master]\n  push:\n"
PUSH_ALLE = "on:\n  pull_request:\n    branches: [master]\n  push:\n    branches: ['**']\n"
PUSH_MASTER = "on:\n  pull_request:\n    branches: [master]\n  push:\n    branches: [master]\n"

r = run(tree({"push.yml": wf(PUSH_MASTER, SAFE)}))
expect("a push filtered to master keeps the expression safe", r.returncode == 0,
       f"exit={r.returncode} {r.stderr[-160:]}")
r = run(tree({"push.yml": wf(PUSH_LOS, SAFE)}))
expect("an UNFILTERED push makes it unsafe", r.returncode == 1, f"exit={r.returncode}")
r = run(tree({"push.yml": wf(PUSH_ALLE, SAFE)}))
expect("  and so does branches: ['**']", r.returncode == 1, f"exit={r.returncode}")

MINIMUM_CASES = 29  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
