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

There are three lanes since #343, and the middle one is the landing: a push to
master builds and tests the crates that landing touches, not the workspace. That
adds a fourth silent decision -- WHICH BASE the selection is measured against --
and the hook is the only place that knows it, because git hands the remote's sha
on the same line it hands the destination. A wrong base compiles the wrong
crates and says so in a summary that looks entirely ordinary, so it is observed
here through the stub rather than read out of the hook.

So the cases here run the real hook against fabricated stdin, with the gate
replaced by a stub that records its argument. Reading the source for the string
`--full` would pass a hook that computes the lane correctly and then never uses
it, which is the failure mode worth catching. The cases at the end are the
exception and say so: they read the GATE for the lane it accepts, because a hook
that passes `--landing` to a gate that rejects it refuses every landing, and no
stub can see that.
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
             message_guard: bool = False,
             extra_env: dict[str, str] | None = None,
             remote_gate: int | None = None,
             keep_repo: bool = False) -> tuple[int, str, str]:
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
        # The base the hook hands over, written whether or not it was set, so
        # "nothing was exported" and "the wrong sha was exported" are different
        # files rather than the same absence.
        f'printf "%s" "${{LANDING_BASE:-<none>}}" > "{tmp}/basis.txt"\n'
        f"exit {gate_exit}\n"
    )
    # THE OTHER PLACE THE GATE CAN RUN (#474). With the remote lane present the
    # hook must call it and not the local gate, and with XFA_GATE_REMOTE=0 it
    # must call the local one even though the script is right there. Both are
    # stubs writing which of them ran, because a hook that computes the choice
    # correctly and then calls the other one is exactly the silent failure this
    # file exists for -- and the wrong direction here spends money on every push
    # or takes the lock on every push, and neither announces itself.
    if remote_gate is not None:
        (tmp / "scripts" / "cos").mkdir(parents=True, exist_ok=True)
        op_afstand = tmp / "scripts" / "cos" / "remote_gate_lane.sh"
        op_afstand.write_text(
            "#!/usr/bin/env bash\n"
            f'printf "%s" "${{1:-<geen>}}" > "{tmp}/afstand.txt"\n'
            f"exit {remote_gate}\n"
        )
        op_afstand.chmod(0o755)

    # The guard the hook runs before the gate reads a revision range against an
    # upstream. There is none here, and the hook says so itself: without one it
    # skips that half. Removing the file keeps this test on its own subject.
    env = sealed_env(cwd=tmp)
    env["PATH"] = os.environ["PATH"]
    # LANDING_BASE and LANE are the hook's own OUTPUT and INPUT, and both are
    # exported by whoever is landing. Inheriting either would test the caller's
    # shell rather than the hook: a run started with LANDING_BASE already set
    # made "the hook exported nothing" indistinguishable from "the hook exported
    # the right thing", which is how this pair of cases first went red.
    # XFA_GATE_REMOTE joins them, and it arrived here the hard way: the gate now
    # runs ON an instance, where remote_gate_lane.sh exports XFA_GATE_REMOTE=0 so
    # that a gate cannot buy a second instance to gate itself. Inherited, that
    # made every case below take the local branch and four of them fail there
    # while passing on the Mac -- green here and red there, from the caller's
    # shell rather than from the hook.
    # GATE_LOCAL_OK joins them for the same reason (#591), and then is SET: the
    # cases about lanes run the local stub on purpose, and the cases about the
    # refusal take it away themselves.
    for leaked in ("LANDING_BASE", "LANE", "XFA_GATE_REMOTE", "GATE_LOCAL_OK"):
        env.pop(leaked, None)
    env["GATE_LOCAL_OK"] = "1"
    env.update(extra_env or {})
    if not keep_repo:
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


def op_afstand(tmp: pathlib.Path) -> str:
    """The lane the remote gate was asked for, or NOT_CALLED."""
    pad = tmp / "afstand.txt"
    return pad.read_text() if pad.exists() else NOT_CALLED


def basis(tmp: pathlib.Path) -> str:
    """The LANDING_BASE the hook exported for that run, or NOT_CALLED."""
    path = tmp / "basis.txt"
    return path.read_text() if path.exists() else NOT_CALLED


def main() -> int:
    import tempfile

    with tempfile.TemporaryDirectory(prefix="lane-") as d:
        tmp = pathlib.Path(d)

        # A push to master takes the LANDING lane (#343): every gate that does
        # not compile, plus a build and test of the crates that landing touches.
        # The whole workspace runs on master, on the runner.
        rc, baan, uit = hook_met(
            "refs/heads/t3/iets abc123 refs/heads/master def456\n", tmp=tmp / "a")
        expect("a push to master runs the landing lane", baan == "--landing", baan)
        expect("and says so before it starts",
               "LANDING local CI gate" in uit, uit[:200])
        # THE BASE IS THE REMOTE'S SHA, not a guess and not the branch's own
        # merge base. Observed through the stub: what the gate is handed decides
        # which files the selection reads, so a wrong base silently compiles the
        # wrong crates.
        expect("and hands the gate the sha master is at",
               basis(tmp / "a") == "def456", basis(tmp / "a"))

        # LANE=full is the on-demand half of the decision, and the one override
        # that costs MORE work rather than less.
        rc, baan, uit = hook_met(
            "refs/heads/t3/iets abc123 refs/heads/master def456\n", tmp=tmp / "a2",
            extra_env={"LANE": "full"})
        expect("LANE=full takes the landing back to the whole workspace",
               baan == "--full", baan)
        expect("and says FULL rather than LANDING",
               "FULL local CI gate" in uit, uit[:200])
        # Only on a landing: LANE=full does not turn an ordinary branch push
        # into a compile nobody asked for on that branch.
        rc, baan, uit = hook_met(
            "refs/heads/t3/iets abc123 refs/heads/t3/iets def456\n", tmp=tmp / "a3",
            extra_env={"LANE": "full"})
        expect("LANE=full leaves a branch push in the fast lane", baan == "--fast", baan)

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
        expect("a detached HEAD pushed to master runs the landing lane",
               baan == "--landing", baan)

        # Several refs at once: one of them landing on master is enough.
        rc, baan, uit = hook_met(
            "refs/heads/a 1 refs/heads/a 2\nrefs/heads/b 3 refs/heads/master 4\n", tmp=tmp / "e")
        expect("one master ref among several picks the landing lane",
               baan == "--landing", baan)
        expect("and the base comes from the master line, not the first one",
               basis(tmp / "e") == "4", basis(tmp / "e"))

        # A master that does not exist yet writes a zero sha. That names no
        # object, so handing it on would make every gate below diff against
        # nothing; the gate finds its own base instead.
        rc, baan, uit = hook_met(
            f"HEAD {SHA} refs/heads/master {ZERO}\n", tmp=tmp / "e2")
        expect("a master that does not exist yet hands over no base",
               basis(tmp / "e2") == "<none>", basis(tmp / "e2"))
        expect("and the lane is still the landing one", baan == "--landing", baan)

        # A branch push hands over no base either: there is no landing to
        # measure, and a stale export would scope a later run to the wrong files.
        rc, baan, uit = hook_met(
            "refs/heads/t3/iets abc123 refs/heads/t3/iets def456\n", tmp=tmp / "e3")
        expect("a branch push hands over no base", basis(tmp / "e3") == "<none>",
               basis(tmp / "e3"))

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

        # A TAG ON A COMMIT MASTER ALREADY HAS STARTS NOTHING EITHER (#541).
        # A release tag publishes no commit, and the commit-message guard reads
        # an empty range for it and refuses -- so `git push --tags` could not
        # pass the gate at all until this branch existed. HEAD is used as the
        # tagged object: it is on this sandbox's master by construction, which
        # is the case the branch is about.
        def met_master(waar: pathlib.Path) -> str:
            """A sandbox whose `github/master` exists, and the commit it names.

            The hook asks whether the tagged object is already on the remote's
            master, and it can only answer that in a repository that has one.
            """
            waar.mkdir(parents=True, exist_ok=True)
            omgeving = sealed_env(cwd=waar)
            omgeving["PATH"] = os.environ["PATH"]
            def g(*a: str) -> str:
                return subprocess.run(["git", *a], cwd=waar, env=omgeving, check=True,
                                      capture_output=True, text=True).stdout.strip()
            g("init", "-q", ".")
            g("-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
              "commit", "-q", "--allow-empty", "-m", "base")
            sha = g("rev-parse", "HEAD")
            g("update-ref", "refs/remotes/github/master", sha)
            return sha

        kop = met_master(tmp / "t1")
        rc, baan, uit = hook_met(
            f"refs/tags/v9.9.9 {kop} refs/tags/v9.9.9 {ZERO}\n", tmp=tmp / "t1")
        expect("a tag on a commit master has does not start the gate",
               baan == NOT_CALLED, baan)
        expect("and the push goes through", rc == 0, str(rc))
        expect("and it says nothing is added", "tags only" in uit, uit[:200])

        # And the direction where a mistake would let code out: a tag on a
        # commit master does NOT have is a way to publish work that never
        # landed, so it takes the whole lane.
        met_master(tmp / "t2")
        rc, baan, uit = hook_met(
            f"refs/tags/v9.9.9 {SHA} refs/tags/v9.9.9 {ZERO}\n", tmp=tmp / "t2")
        expect("a tag on a commit that is not on master runs the gate",
               baan == "--fast", baan)

        # A tag beside a branch update is not a tag-only push.
        # AND WITHOUT A MASTER TO COMPARE WITH, THE ANSWER IS NOT YES (codex,
        # #1896). A fresh clone, or one whose remote is called something else,
        # has no refs/remotes/github/master; falling back to HEAD there made a
        # tag on the current unlanded commit its own ancestor.
        (tmp / "t4").mkdir(parents=True, exist_ok=True)
        rc, baan, uit = hook_met(
            f"refs/tags/v9.9.9 {kop} refs/tags/v9.9.9 {ZERO}\n", tmp=tmp / "t4")
        expect("no github/master to compare with runs the gate", baan == "--fast", baan)

        kop3 = met_master(tmp / "t3")
        rc, baan, uit = hook_met(
            f"refs/tags/v9.9.9 {kop3} refs/tags/v9.9.9 {ZERO}\n"
            f"refs/heads/b {SHA} refs/heads/b {SHA}\n", tmp=tmp / "t3")
        expect("a tag mixed with a branch update runs the gate", baan == "--fast", baan)
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
        # exist in the file, or "deferred to the landing lane" defers nothing.
        tekst = GATE.read_text()
        zwaar = [r for r in tekst.splitlines()
                 if r.startswith(("zwaar ", "scoped ", "crate_gate "))]
        expect("the gate file defers at least four heavy gates",
               len(zwaar) >= 4, f"{len(zwaar)}")
        expect("a deferred gate is not counted as a pass",
               "DEFERRED (not a pass)" in tekst)
        expect("nor is a gate skipped for touching no crate",
               "SKIPPED (not a pass)" in tekst)
        expect("the summary names the lane it ran",
               '$_lane lane' in tekst)
        # THE LANE EXISTS IN THE GATE, not only in the hook. The hook passing
        # `--landing` to a gate that rejects it would refuse every landing, and
        # the case above cannot see that: it runs against a stub.
        expect("the gate accepts the lane the hook passes it",
               "--landing)" in tekst, "")
        expect("and LANE=full overrides it there too",
               'if [ "${LANE:-}" = full ]' in tekst)
        # The three that take the crate list. A `scoped` line that lost its
        # crates would compile the whole workspace again, silently and slowly;
        # one that lost its command would compile nothing at all.
        for gate in ("scoped build", "scoped clippy", "scoped test"):
            expect(f"`{gate}` runs over the selection",
                   any(r.startswith(gate) for r in tekst.splitlines()), gate)
        expect("the selection comes from touched_crates.py",
               "scripts/ci/touched_crates.py" in tekst)
        # AND ITS FAILURE IS A REFUSAL TO NARROW, not a quiet narrowing. If the
        # selection cannot be made, the lane has to widen to the whole
        # workspace -- the direction that costs time rather than certainty.
        expect("a selection that cannot be made builds everything",
               "FULL=1; LANDING=0" in tekst)

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
        expect("operational history is advisory and only fetched in the full lane",
               'if [ "$PLAATS" != machine ] && [ "$FULL" = 1 ]; then\necho "--- groen' in tekst
               and any(r.startswith("python3 scripts/ci/a_gate_that_never_went_green.py")
                       and r.rstrip().endswith("|| true") for r in regels))
        # Its test is not advisory anywhere: it is local and deterministic, it
        # asks no API, and it is what proves the guard can go red at all.
        expect("its own test stays blocking in both lanes",
               any(r.startswith("run groentest") for r in regels))

    # WHERE THE GATE RUNS (#474). Four cases, because each direction is wrong in
    # a way nobody would notice from a green push: a hook that always goes
    # remote spends money on a machine that is already idle, and one that never
    # does puts every push back in the queue that cost 2-3.5 hours a day.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        rc, baan, uit = hook_met(f"refs/heads/x {SHA} refs/heads/x {ZERO}\n",
                                 tmp=tmp, remote_gate=0, extra_env={"GATE_LOCAL_OK": ""})
        # THE SILENT FALLBACK (#591). Two landings ran their lane on the Mac for
        # fifty-four minutes each because nothing set XFA_GATE_REMOTE and the
        # hook took that as "run it here". Unasked, it must run nothing and say so.
        expect("with no choice made, no gate runs on this machine",
               op_afstand(tmp) == NOT_CALLED and baan == NOT_CALLED,
               f"remote={op_afstand(tmp)!r} local={baan!r}")
        expect("and the push is refused", rc != 0, str(rc))
        expect("with the reason and the two ways out",
               "REFUSED TO RUN ON THIS MAC" in uit and "XFA_GATE_REMOTE is" in uit
               and "GATE_LOCAL_OK=1" in uit and "NOTHING WAS CHECKED" in uit, uit[-300:])
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        rc, baan, uit = hook_met(f"refs/heads/x {SHA} refs/heads/x {ZERO}\n",
                                 tmp=tmp, remote_gate=0)
        expect("GATE_LOCAL_OK=1 runs the gate here, and says so",
               baan == "--fast" and op_afstand(tmp) == NOT_CALLED and rc == 0
               and "GATE ON THIS MAC, as asked" in uit,
               f"rc={rc} local={baan!r} remote={op_afstand(tmp)!r}")
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        hook_met(f"refs/heads/master {SHA} refs/heads/master {ZERO}\n",
                 tmp=tmp, remote_gate=0, extra_env={"XFA_GATE_REMOTE": "1"})
        expect("an explicit remote landing uses the landing lane", op_afstand(tmp) == "--landing",
               repr(op_afstand(tmp)))
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        rc, baan, _uit = hook_met(f"refs/heads/x {SHA} refs/heads/x {ZERO}\n",
                                  tmp=tmp, remote_gate=0,
                                  extra_env={"XFA_GATE_REMOTE": "0"})
        expect("XFA_GATE_REMOTE=0 keeps the gate on this machine",
               baan == "--fast" and op_afstand(tmp) == NOT_CALLED,
               f"local={baan!r} remote={op_afstand(tmp)!r}")
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        rc, baan, uit = hook_met(f"refs/heads/master {SHA} refs/heads/master {ZERO}\n",
                                 tmp=tmp, remote_gate=0,
                                 extra_env={"XFA_GATE_REMOTE": "0", "GATE_LOCAL_OK": "0"})
        expect("XFA_GATE_REMOTE=0 without GATE_LOCAL_OK=1 refuses a landing too",
               rc != 0 and baan == NOT_CALLED and op_afstand(tmp) == NOT_CALLED
               and "REFUSED TO RUN ON THIS MAC" in uit,
               f"rc={rc} local={baan!r} remote={op_afstand(tmp)!r}")
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        # A remote gate that could not run is not a pass, and the hook must not
        # let the push through on it. 76 is "no evidence the command ran", 75 is
        # "no instance and no fallback"; both mean nothing was checked.
        rc, _baan, uit = hook_met(f"refs/heads/x {SHA} refs/heads/x {ZERO}\n",
                                  tmp=tmp, remote_gate=75, extra_env={"XFA_GATE_REMOTE": "1"})
        expect("a remote gate that bought nothing blocks the push", rc != 0, str(rc))
        expect("and says nothing was checked", "NOTHING WAS CHECKED" in uit, uit[-200:])

    # A PUSH THAT TOUCHES NO CRATE NEVER BUYS A MACHINE (owner, 23-09-2026).
    # A real repository here, because the rule reads a real diff: a base
    # commit that github/master points at, and one commit on top.
    def no_crate_case(changed: str, also: str | None = None) -> tuple[int, str, str, str]:
        with tempfile.TemporaryDirectory() as d:
            tmp = pathlib.Path(d)
            env = sealed_env(cwd=tmp)
            env["PATH"] = os.environ["PATH"]
            for leaked in ("LANDING_BASE", "LANE", "XFA_GATE_REMOTE", "GATE_LOCAL_OK"):
                env.pop(leaked, None)
            def g(*a: str) -> str:
                return subprocess.run(["git", "-c", "user.name=f", "-c", "user.email=f@example.invalid", *a],
                                      cwd=tmp, env=env, check=True, capture_output=True,
                                      text=True).stdout.strip()
            g("init", "-q", ".")
            (tmp / "README.md").write_text("base\n")
            g("add", "-A"); g("commit", "-q", "-m", "base")
            g("update-ref", "refs/remotes/github/master", "HEAD")
            target = tmp / changed
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("changed\n")
            g("add", "-A"); g("commit", "-q", "-m", "the change")
            head = g("rev-parse", "HEAD")
            lines = f"refs/heads/x {head} refs/heads/x {ZERO}\n"
            if also is not None:
                (tmp / "docs").mkdir(exist_ok=True)
                (tmp / "docs" / "note.md").write_text("docs\n")
                g("checkout", "-q", "-b", "docs-only", "refs/remotes/github/master")
                g("add", "-A"); g("commit", "-q", "-m", "docs only")
                other = g("rev-parse", "HEAD")
                lines = (f"refs/heads/x {head} refs/heads/x {ZERO}\n"
                         f"refs/heads/docs-only {other} refs/heads/docs-only {ZERO}\n")
            rc, baan, uit = hook_met(lines,
                                     tmp=tmp, remote_gate=0,
                                     extra_env={"XFA_GATE_REMOTE": "1", "GATE_LOCAL_OK": ""},
                                     keep_repo=True)
            return rc, baan, op_afstand(tmp), uit
    rc, baan, afstand, uit = no_crate_case("scripts/ci/some_guard.py")
    expect("a push that touches no crate gates here and buys nothing, though XFA_GATE_REMOTE=1",
           rc == 0 and baan == "--fast" and afstand == NOT_CALLED and "buys nothing" in uit,
           f"rc={rc} local={baan!r} remote={afstand!r}")
    rc, baan, afstand, uit = no_crate_case("crates/pdf-engine/src/lib.rs")
    expect("a push that touches a crate still takes the instance",
           afstand == "--fast" and baan == NOT_CALLED, f"local={baan!r} remote={afstand!r}")
    rc, baan, afstand, uit = no_crate_case("Cargo.lock")
    expect("and so does one that moves the lock file", afstand == "--fast",
           f"local={baan!r} remote={afstand!r}")
    rc, baan, afstand, uit = no_crate_case("tools/pdfluent-snippet-extract/src/main.rs")
    expect("and one that changes a workspace member under tools/ (codex, #1954)",
           afstand == "--fast", f"local={baan!r} remote={afstand!r}")

    rc, baan, afstand, uit = no_crate_case("crates/pdf-engine/src/lib.rs", also="docs")
    expect("a crate on one ref and docs on another still takes the instance (codex, #1954)",
           afstand == "--fast", f"local={baan!r} remote={afstand!r}")

    print(f"[test-lane] {ran - len(fails)}/{ran} case(s) ok")
    if fails:
        print("[test-lane] FAIL: " + "; ".join(fails))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
