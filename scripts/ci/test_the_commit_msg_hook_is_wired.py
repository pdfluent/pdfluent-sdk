#!/usr/bin/env python3
"""The hook guard's three layers, and where each is allowed to be judged (#1660).

Layer 3 asks whether `core.hooksPath` is set in THIS clone. A CI checkout never
has it, so the job was red on every pull request for a reason that had nothing
to do with the pull request. `--alleen-boom` lets the caller say it cannot
judge that layer.

The danger in a flag like that is obvious and is the thing this file exists to
stop: a skip that becomes a pass everywhere. So two things are asserted
together -- the flag really does stop judging layer 3, AND the local gate still
calls the guard with no flag, which is the one place the answer means anything.
A skipped check needs a home, and this is what checks it still has one.
"""
from __future__ import annotations
import pathlib, os, shutil, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402

GUARD = "scripts/ci/the_commit_msg_hook_is_wired.py"
GIT = "/usr/bin/git"

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def build(hook_text: str | None, executable: bool = True, hooks_path: str | None = None):
    """A repo carrying a .githooks/commit-msg, and a clone that may or may not use it."""
    td = tempfile.mkdtemp()
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / ".githooks").mkdir()
    for name in ("the_commit_msg_hook_is_wired.py", "fixture_env.py"):
        shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
    if hook_text is not None:
        haak = root / ".githooks" / "commit-msg"
        haak.write_text(hook_text)
        haak.chmod(0o755 if executable else 0o644)
    env = sealed_env(identity=True, cwd=root)
    subprocess.run([GIT, "init", "-q"], cwd=root, env=env, check=True)
    if hooks_path is not None:
        subprocess.run([GIT, "config", "core.hooksPath", hooks_path],
                       cwd=root, env=env, check=True)
    return root, env


def run(root, env, *args):
    return subprocess.run([sys.executable, str(root / GUARD), *args],
                          cwd=root, env=env, capture_output=True, text=True)


GOED = """#!/bin/sh
python3 scripts/ci/no_ai_attribution.py "$1" || exit 1
python3 scripts/ci/geen_interne_zaken.py "$1" || exit 1
"""

# A hook whose CALL is gone but whose comment still names the guard: the shape
# a plain substring search passes.
COMMENTAAR_ALLEEN = """#!/bin/sh
# This hook used to call geen_interne_zaken.py and no longer does.
python3 scripts/ci/no_ai_attribution.py "$1" || exit 1
"""

print("the commit-msg hook guard — layers, and where each may be judged")

# Layer 3 alone: the tree is right, the clone is not.
root, env = build(GOED, hooks_path=None)
r = run(root, env)
expect("an unconfigured clone fails the full check", r.returncode == 1,
       f"exit={r.returncode}: {r.stderr[:160]}")
r = run(root, env, "--alleen-boom")
expect("  and passes --alleen-boom", r.returncode == 0,
       f"exit={r.returncode}: {r.stderr[:160]}")
expect("  which SAYS layer 3 was not judged",
       "NOT judged" in r.stdout and "local_ci_gate" in r.stdout, r.stdout[:200])

root, env = build(GOED, hooks_path=".githooks")
r = run(root, env)
expect("a configured clone passes the full check", r.returncode == 0,
       f"exit={r.returncode}: {r.stderr[:160]}")

# The tree layers must still bite BEHIND the flag -- otherwise the flag is an
# off switch rather than a narrower question.
for naam, tekst, uitvoerbaar in (
        ("a missing hook", None, True),
        ("a hook that lost a call", COMMENTAAR_ALLEEN, True),
        ("a non-executable hook", GOED, False)):
    root, env = build(tekst, executable=uitvoerbaar, hooks_path=".githooks")
    r = run(root, env, "--alleen-boom")
    expect(f"{naam} fails even with --alleen-boom", r.returncode == 1,
           f"exit={r.returncode}")

# A comment naming the guard does not stand in for calling it.
root, env = build(COMMENTAAR_ALLEEN, hooks_path=".githooks")
r = run(root, env, "--alleen-boom")
expect("  and the comment naming it is not mistaken for the call",
       "geen_interne_zaken" in r.stderr, r.stderr[:200])

# THE HOME. Layer 3 is skipped in CI only because somewhere else still asks it.
#
# Matched on the exact script ARGUMENT, not as a substring. The first version
# asked whether "the_commit_msg_hook_is_wired.py" appeared anywhere in the
# gate -- and `test_the_commit_msg_hook_is_wired.py`, the line that runs THIS
# file, contains it. So deleting the guard's own invocation left the suite
# green: the assertion that the skipped layer still had a home was satisfied by
# the test asserting it. (codex, #1660)
def gate_aanroepen(script: str) -> list[str]:
    """Lines of the local gate that run exactly this script."""
    uit = []
    for regel in (REPO / "scripts" / "ci" / "local_ci_gate.sh").read_text().splitlines():
        for stuk in regel.split():
            if stuk.rsplit("/", 1)[-1] == script:
                uit.append(regel)
                break
    return uit


aanroepen = gate_aanroepen("the_commit_msg_hook_is_wired.py")
expect("the local gate still calls the guard itself", len(aanroepen) == 1,
       f"matched {len(aanroepen)} line(s): {aanroepen}")
expect("  and calls it with NO flag, so layer 3 keeps a place to fail",
       aanroepen and all("--alleen-boom" not in r for r in aanroepen), str(aanroepen))
expect("  and the test is wired separately",
       len(gate_aanroepen("test_the_commit_msg_hook_is_wired.py")) == 1)

MINIMUM_CASES = 11  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
