#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The sweep exemption, and above all the ways it must not work.

This code runs a command named in a commit message. That is exactly the kind of
path that deserves a test before it runs anywhere, and case 2 is the reason:
unless the authorisation comes from the BASE, a branch authorises itself.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

GUARD = CI / "territories_do_not_overlap.py"

MAP_TOML = '''[[territory]]
id = "t1"
name = "Ander"
paden = ["owned/**"]

[[territory]]
id = "t3"
name = "Mij"
paden = ["mine/**"]
'''
SWEEPS_SECTION = '\n[sweeps]\ntoegestaan = ["python3 scripts/sweep.py"]\n'

# A sweep that does something real and is idempotent: it puts a fixed line at
# the top of every file in owned/, and leaves it alone when it is already there.
SWEEP = '''#!/usr/bin/env python3
import pathlib
for f in sorted(pathlib.Path("owned").glob("*.txt")):
    t = f.read_text()
    if not t.startswith("# header\\n"):
        f.write_text("# header\\n" + t)
'''


def git(*a, cwd, **kw):
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True, text=True,
                          env=sealed_env(cwd=cwd), **kw)


def repo(tmp: pathlib.Path, allowlist_on_base: bool) -> pathlib.Path:
    """A repository with a base (master) and a branch on top of it."""
    r = tmp / "repo"
    (r / ".claude").mkdir(parents=True)
    (r / "scripts" / "ci").mkdir(parents=True)
    (r / "owned").mkdir()
    (r / "mine").mkdir()
    shutil.copy(GUARD, r / "scripts" / "ci" / GUARD.name)
    shutil.copy(CI / "fixture_env.py", r / "scripts" / "ci" / "fixture_env.py")
    (r / "scripts" / "sweep.py").write_text(SWEEP)
    (r / ".claude" / "territories.toml").write_text(
        MAP_TOML + (SWEEPS_SECTION if allowlist_on_base else ""))
    for i in range(3):
        (r / "owned" / f"a{i}.txt").write_text(f"line {i}\n")
    (r / "mine" / "own.txt").write_text("mine\n")
    git("init", "-q", "-b", "master", cwd=r)
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "basis", cwd=r)
    git("checkout", "-q", "-b", "t3/sweep", cwd=r)
    return r


def sweep_commit(r: pathlib.Path, command: str, hand_edit: str = "") -> None:
    subprocess.run([sys.executable, "scripts/sweep.py"], cwd=str(r), check=True)
    if hand_edit == "geraakt":
        # The generator manages the header line, so it undoes this.
        (r / "owned" / "a0.txt").write_text("# other header\nline 0\n")
    elif hand_edit == "ongeraakt":
        # A file the generator never touches. Re-running does NOT notice this,
        # and that is the limit of the mechanism, not a fault in it.
        (r / "owned" / "by-hand.md").write_text("nobody generated this\n")
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", f"sweep\n\nGenerated-by: {command}", cwd=r)


def ask_guard(r: pathlib.Path) -> tuple[set, str | None]:
    """Call the guard inside the temporary repository, with its own ROOT."""
    code = ("import sys; sys.path.insert(0, 'scripts/ci');"
            "import territories_do_not_overlap as T;"
            "v, reason = T.sweep_exemption('master');"
            "print(len(v)); print(reason or '')")
    out = subprocess.run([sys.executable, "-c", code], cwd=str(r),
                         capture_output=True, text=True, env=sealed_env(cwd=r))
    if out.returncode != 0:
        return set(), f"<crash> {out.stderr.strip()[-200:]}"
    n, reason = out.stdout.split("\n", 1)
    return set(range(int(n))), (reason.strip() or None)


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FOUT'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    ok_all = True

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), allowlist_on_base=True)
        sweep_commit(r, "python3 scripts/sweep.py")
        freed, reason = ask_guard(r)
        ok_all &= case("an allowed sweep that reproduces frees its files",
                      reason is None and len(freed) == 3, f"{len(freed)} freed, reason={reason}")

    # THE case. The branch puts its own command in its own map. If the allowlist
    # came from the worktree, the branch would authorise itself -- and this guard
    # runs before every review, so "the reviewer sees it there too" comes too
    # late.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), allowlist_on_base=False)
        map_file = r / ".claude" / "territories.toml"
        map_file.write_text(map_file.read_text() + SWEEPS_SECTION)
        git("add", "-A", cwd=r)
        git("commit", "-q", "-m", "de tak machtigt zichzelf", cwd=r)
        sweep_commit(r, "python3 scripts/sweep.py")
        freed, reason = ask_guard(r)
        ok_all &= case("a branch cannot authorise itself through its own map",
                      not freed, f"{len(freed)} file(s) freed, reason={reason}")

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), allowlist_on_base=True)
        sweep_commit(r, "python3 scripts/iets_anders.py")
        freed, reason = ask_guard(r)
        ok_all &= case("a command outside the allowlist is refused",
                      not freed and reason and "toegestaan" in reason, f"reason={reason}")

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), allowlist_on_base=True)
        sweep_commit(r, "python3 scripts/sweep.py", hand_edit="geraakt")
        freed, reason = ask_guard(r)
        ok_all &= case("a hand edit inside what the generator manages is caught",
                      not freed and reason and "not purely mechanical" in reason, f"reason={reason}")

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), allowlist_on_base=True)
        sweep_commit(r, "python3 scripts/sweep.py")
        # A TRACKED file with unsaved changes: that is what a re-run plus
        # `git checkout -- .` would throw away.
        (r / "mine" / "own.txt").write_text("work that exists nowhere else yet\n")
        freed, reason = ask_guard(r)
        ok_all &= case("a dirty worktree is SKIPPED, not green and not cleaned up",
                      not freed and reason and "SKIPPED" in reason, f"reason={reason}")
        ok_all &= case("the unsaved work has not been wiped",
                      (r / "mine" / "own.txt").read_text().startswith("work that"), "")

    # This WAS the known limit: "re-running gives an empty diff" proves the
    # generated parts are generated, but not that nothing else rode along -- a
    # file the generator never touches reproduces cleanly.
    #
    # T2 pointed out on #1700 that the limit can be closed by intersecting the
    # exemption with what the generator WRITES, measured by running it on the
    # parent of the sweep commit. That is behaviour now rather than a footnote,
    # and this case guards it.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), allowlist_on_base=True)
        sweep_commit(r, "python3 scripts/sweep.py", hand_edit="ongeraakt")
        freed, reason = ask_guard(r)
        ok_all &= case("a hand edit outside the generator reach is caught",
                      not freed and reason and "added by" in reason,
                      f"{len(freed)} freed, reason={reason}")

    print("test_sweep_exemption: " + ("OK" if ok_all else "GEFAALD"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
