#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Two-way proof for the pre-push hook's lane choice and its disk refusal.

The hook decides two things that a reader cannot check by looking at a push that
went through: WHICH set of gates ran, and WHAT a refusal meant. Both are silent
when they are wrong -- a push that ran the fast lane and a push that ran the full
one print a different count and nothing else, and before this the disk floor and
a genuine gate failure printed the same sentence.

The third decision is WHETHER ANY of them run. A push whose refspec only deletes
remote refs adds nothing, so nothing the gates measure has changed -- and until
#325 such a push compiled the whole workspace anyway. Getting that wrong in the
permissive direction skips the build on a real push, so every input that is NOT
a deletion is asserted here as carefully as the one that is.

So the cases here run the real hook against fabricated stdin, with the gate
replaced by a stub that records its argument. Reading the source for the string
`--full` would pass a hook that computes the lane correctly and then never uses
it, which is the failure mode worth catching.
"""
from __future__ import annotations
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / "scripts" / "ci"
HOOK = REPO / ".githooks" / "pre-push"
GATE = CI / "local_ci_gate.sh"
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

# The gate stub writes this file when it runs; its absence is what "the build
# never started" means, and it is asserted on rather than only printed.
NOT_CALLED = "<not called>"
ZERO = "0" * 40
SHA = "1" * 40

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def hook_met(stdin: str, gate_exit: int = 0, tmp: pathlib.Path | None = None,
             message_guard: bool = False) -> tuple[int, str, str]:
    """Run the real hook in a sandbox whose local_ci_gate.sh is a stub.

    The stub writes its argument to a file and exits with `gate_exit`, so the
    lane is observed rather than inferred, and the disk path can be produced
    without filling a disk.

    With `message_guard`, a second stub is planted where the commit-message
    guard lives, refusing everything and leaving a file behind when it runs.
    The hook only reaches it when the branch has an upstream, so the sandbox
    gets one: a commit, a remote-tracking ref for it, and the two config keys
    that make `@{u}` resolve. That turns "the guard was skipped" into something
    observed rather than argued from the order of the lines.
    """
    assert tmp is not None
    (tmp / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
    (tmp / ".githooks").mkdir(parents=True, exist_ok=True)
    (tmp / ".githooks" / "pre-push").write_text(HOOK.read_text())
    (tmp / "scripts" / "ci" / "local_ci_gate.sh").write_text(
        "#!/usr/bin/env bash\n"
        f'printf "%s" "${{1:-<geen>}}" > "{tmp}/baan.txt"\n'
        f"exit {gate_exit}\n"
    )
    # The guard the hook runs before the gate reads a revision range against an
    # upstream. There is none here, and the hook says so itself: without one it
    # skips that half. Removing the file keeps this test on its own subject.
    env = sealed_env(cwd=tmp)
    env["PATH"] = os.environ["PATH"]
    subprocess.run(["git", "init", "-q", "."], cwd=tmp, env=env, check=True)
    if message_guard:
        (tmp / "scripts" / "ci" / "geen_interne_zaken.py").write_text(
            "import pathlib, sys\n"
            f'pathlib.Path(r"{tmp}/guard-ran.txt").write_text("yes")\n'
            "sys.exit(1)\n"
        )
        def g(*a: str) -> str:
            r = subprocess.run(["git", *a], cwd=tmp, env=env, check=True,
                               capture_output=True, text=True)
            return r.stdout.strip()

        g("-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
          "commit", "-q", "--allow-empty", "-m", "base")
        branch = g("rev-parse", "--abbrev-ref", "HEAD")
        g("update-ref", f"refs/remotes/origin/{branch}", "HEAD")
        g("config", "remote.origin.url", ".")
        g("config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*")
        g("config", f"branch.{branch}.remote", "origin")
        g("config", f"branch.{branch}.merge", f"refs/heads/{branch}")
    r = subprocess.run(
        ["bash", str(tmp / ".githooks" / "pre-push")],
        cwd=tmp, input=stdin, capture_output=True, text=True, env=env,
    )
    baan = (tmp / "baan.txt").read_text() if (tmp / "baan.txt").exists() else NOT_CALLED
    return r.returncode, baan, r.stdout + r.stderr


def main() -> int:
    import tempfile

    with tempfile.TemporaryDirectory(prefix="lane-") as d:
        tmp = pathlib.Path(d)

        # A push to master takes the full lane.
        rc, baan, uit = hook_met(
            "refs/heads/t3/iets abc123 refs/heads/master def456\n", tmp=tmp / "a")
        expect("a push to master runs the full lane", baan == "--full", baan)
        expect("and says so before it starts", "FULL local CI gate" in uit, uit[:120])

        # A push to any other ref does not.
        rc, baan, uit = hook_met(
            "refs/heads/t3/iets abc123 refs/heads/t3/iets def456\n", tmp=tmp / "b")
        expect("a push to a branch runs the fast lane", baan == "--fast", baan)

        # The branch NAME is not the signal. A branch called master pushed
        # somewhere else is not a landing.
        rc, baan, uit = hook_met(
            "refs/heads/master abc123 refs/heads/t3/proef def456\n", tmp=tmp / "c")
        expect("a branch named master pushed elsewhere stays fast", baan == "--fast", baan)

        # And a detached HEAD landing on master is one, which is how ff3 pushes.
        rc, baan, uit = hook_met(
            "HEAD abc123 refs/heads/master def456\n", tmp=tmp / "d")
        expect("a detached HEAD pushed to master runs the full lane", baan == "--full", baan)

        # Several refs at once: one of them landing on master is enough.
        rc, baan, uit = hook_met(
            "refs/heads/a 1 refs/heads/a 2\nrefs/heads/b 3 refs/heads/master 4\n", tmp=tmp / "e")
        expect("one master ref among several picks the full lane", baan == "--full", baan)

        # A DELETE-ONLY PUSH STARTS NOTHING. git writes a local sha that names
        # no object for a deletion, so the gate is never invoked at all -- and
        # the stub not having been called is the proof that no build began.
        rc, baan, uit = hook_met(
            f"(delete) {ZERO} refs/heads/dood {SHA}\n", tmp=tmp / "h")
        expect("a delete-only push does not start the gate", baan == NOT_CALLED, baan)
        expect("and the push goes through", rc == 0, str(rc))
        expect("and it says why it ran nothing", "deletions only" in uit, uit[:160])

        rc, baan, uit = hook_met(
            f"(delete) {ZERO} refs/heads/a {SHA}\n"
            f"(delete) {ZERO} refs/heads/b {SHA}\n"
            f"(delete) {ZERO} refs/heads/c {SHA}\n", tmp=tmp / "i")
        expect("three deletions are still delete-only", baan == NOT_CALLED, baan)
        expect("and the count is the one git handed over", "3 ref(s)" in uit, uit[:160])

        # One ref carrying objects is enough for the build to have something to
        # measure, in either order. This is the direction where a mistake is
        # expensive: it would skip the gate on a real push.
        rc, baan, uit = hook_met(
            f"(delete) {ZERO} refs/heads/a {SHA}\n"
            f"refs/heads/b {SHA} refs/heads/b {SHA}\n", tmp=tmp / "j")
        expect("a deletion mixed with an update is not delete-only", baan == "--fast", baan)
        rc, baan, uit = hook_met(
            f"refs/heads/b {SHA} refs/heads/b {SHA}\n"
            f"(delete) {ZERO} refs/heads/a {SHA}\n", tmp=tmp / "k")
        expect("the update being first does not change that", baan == "--fast", baan)

        # Width-independent, because "names no object" is spelled the same at
        # every hash width -- a sha-256 repository writes sixty-four zeros.
        rc, baan, uit = hook_met(
            f"(delete) {'0' * 64} refs/heads/a {SHA}\n", tmp=tmp / "l")
        expect("a 64-character zero sha still reads as a deletion",
               baan == NOT_CALLED, baan)
        # And it is not a prefix test: a real commit may begin with a zero.
        rc, baan, uit = hook_met(
            f"refs/heads/a {'0' * 39}1 refs/heads/a {SHA}\n", tmp=tmp / "m")
        expect("a sha that merely starts with zeros is not a deletion",
               baan == "--fast", baan)

        # NO LINES IS NOT A DELETION. Not understanding the input has to cost
        # more work, never less.
        rc, baan, uit = hook_met("", tmp=tmp / "n")
        expect("empty stdin still runs the gate", baan == "--fast", baan)

        # The one thing a deletion can still be refused for.
        rc, baan, uit = hook_met(
            f"(delete) {ZERO} refs/heads/master {SHA}\n", tmp=tmp / "o")
        expect("deleting master is refused", rc == 1, str(rc))
        expect("and refused by name", "deletes refs/heads/master" in uit, uit[-200:])
        expect("without starting the gate to decide it", baan == NOT_CALLED, baan)

        # The commit-message guard reads `@{u}..HEAD` -- the commits of whatever
        # branch is checked out, which on a delete-only push are not the commits
        # being pushed, because there are none. Observed, not argued from the
        # order of the lines: the stub refuses everything and leaves a file when
        # it runs.
        rc, baan, uit = hook_met(
            f"refs/heads/a {SHA} refs/heads/a {SHA}\n", tmp=tmp / "p",
            message_guard=True)
        expect("the message guard blocks an ordinary push", rc == 1, str(rc))
        expect("and ran to do it", (tmp / "p" / "guard-ran.txt").exists())
        rc, baan, uit = hook_met(
            f"(delete) {ZERO} refs/heads/a {SHA}\n", tmp=tmp / "q",
            message_guard=True)
        expect("a delete-only push is not judged on unrelated messages", rc == 0, uit[-200:])
        expect("because the message guard never ran",
               not (tmp / "q" / "guard-ran.txt").exists())

        # Exit 3 is the disk floor and must not be reported as a failing gate.
        rc, baan, uit = hook_met(
            "refs/heads/x 1 refs/heads/x 2\n", gate_exit=3, tmp=tmp / "f")
        expect("the disk floor still blocks the push", rc == 1, str(rc))
        expect("the disk floor says nothing was checked",
               "NOTHING WAS CHECKED" in uit, uit[-160:])
        expect("the disk floor does not claim the gate failed",
               "local CI gate failed" not in uit, uit[-160:])

        # An ordinary failure keeps saying what it always said.
        rc, baan, uit = hook_met(
            "refs/heads/x 1 refs/heads/x 2\n", gate_exit=1, tmp=tmp / "g")
        expect("a failing gate still blocks", rc == 1, str(rc))
        expect("a failing gate is still named as one",
               "local CI gate failed" in uit, uit[-160:])

        # And the lanes are not empty on either side: the deferred gates have to
        # exist in the file, or "deferred to the full lane" defers nothing.
        tekst = GATE.read_text()
        zwaar = [r for r in tekst.splitlines() if r.startswith("zwaar ")]
        expect("the gate file defers at least four compiling gates",
               len(zwaar) >= 4, f"{len(zwaar)}")
        expect("a deferred gate is not counted as a pass",
               "DEFERRED (not a pass)" in tekst)
        expect("the summary names the lane it ran",
               '$_lane lane' in tekst)

        # The never-green guard is advisory in BOTH local lanes since #331: the
        # full lane IS the landing, so leaving it hard there meant a workflow
        # somebody else broke this morning closed master for everyone -- four
        # times in 24 hours, the last on publication-guards.yml. It refuses only
        # in ci.yml's own run on master, where it decides nothing about whether
        # another terminal can land.
        #
        # This is a STRUCTURE check, not a behaviour one: it reads where the
        # invocation sits rather than running the gate, so it would not catch a
        # branch that is present and unreachable. What it does catch is the
        # regression that matters -- the invocation quietly becoming a `run` line
        # again, which is how the previous advisory lines were lost.
        regels = tekst.splitlines()
        try:
            i = next(n for n, r in enumerate(regels)
                     if r.strip().startswith("run groen ")
                     or r.strip().startswith("run groen\t"))
        except StopIteration:
            i = None
        expect("the never-green guard refuses in neither local lane",
               i is None, f"regel {i}")
        expect("and warns in both lanes instead",
               "advisory in both local lanes" in tekst
               and any(r.startswith("python3 scripts/ci/a_gate_that_never_went_green.py")
                       and r.rstrip().endswith("|| true") for r in regels))
        # Its test is not advisory anywhere: it is local and deterministic, it
        # asks no API, and it is what proves the guard can go red at all.
        expect("its own test stays blocking in both lanes",
               any(r.startswith("run groentest") for r in regels))

    print(f"[test-lane] {ran - len(fails)}/{ran} case(s) ok")
    if fails:
        print("[test-lane] FAIL: " + "; ".join(fails))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
