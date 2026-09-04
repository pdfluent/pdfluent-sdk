#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Two-way proof for the pre-push hook's lane choice and its disk refusal.

The hook decides two things that a reader cannot check by looking at a push that
went through: WHICH set of gates ran, and WHAT a refusal meant. Both are silent
when they are wrong -- a push that ran the fast lane and a push that ran the full
one print a different count and nothing else, and before this the disk floor and
a genuine gate failure printed the same sentence.

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

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def hook_met(stdin: str, gate_exit: int = 0, tmp: pathlib.Path | None = None) -> tuple[int, str, str]:
    """Run the real hook in a sandbox whose local_ci_gate.sh is a stub.

    The stub writes its argument to a file and exits with `gate_exit`, so the
    lane is observed rather than inferred, and the disk path can be produced
    without filling a disk.
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
    r = subprocess.run(
        ["bash", str(tmp / ".githooks" / "pre-push")],
        cwd=tmp, input=stdin, capture_output=True, text=True, env=env,
    )
    baan = (tmp / "baan.txt").read_text() if (tmp / "baan.txt").exists() else "<niet aangeroepen>"
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

        # The guards that read the outside world are advisory in the fast lane.
        # This is a STRUCTURE check, not a behaviour one: it reads where the
        # invocation sits rather than running the gate, so it would not catch a
        # branch that is present and unreachable. What it does catch is the
        # regression that matters -- the invocation quietly moving back out of
        # the lane test, which is how the previous advisory lines were lost.
        regels = tekst.splitlines()
        try:
            i = next(n for n, r in enumerate(regels)
                     if r.strip().startswith("run groen ")
                     or r.strip().startswith("run groen\t"))
        except StopIteration:
            i = None
        onder_full = False
        if i is not None:
            for r in reversed(regels[:i]):
                if r.startswith('if [ "$FULL" = 1 ]'):
                    onder_full = True
                    break
                if r.startswith("run ") or r.startswith("zwaar "):
                    break
        expect("the never-green guard refuses only in the full lane",
               i is not None and onder_full, f"regel {i}")
        expect("and warns in the fast lane instead",
               "advisory in the fast lane" in tekst)
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
