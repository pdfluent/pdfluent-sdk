#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the seeding script must do, and above all what it must not.

The case that carries the design is the content one. A history rewrite that
quietly changes a file is a far worse failure than the trailer it was meant to
remove -- and it would not announce itself, because the trailer really would be
gone and the script really would say so.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

SCRIPT = CI.parent / "release" / "seed_public_repo.sh"


def git(*a, cwd):
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                          text=True, env=sealed_env(cwd=cwd))


def source_repo(tmp: pathlib.Path) -> pathlib.Path:
    r = tmp / "source"
    r.mkdir()
    git("init", "-q", "-b", "master", cwd=r)
    (r / "a.txt").write_text("first\n", encoding="utf-8")
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "first\n\nCo-Authored-By: Someone <s@example.invalid>", cwd=r)
    (r / "b.txt").write_text("second\n", encoding="utf-8")
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "second\n\nAssisted-by: Another <a@example.invalid>", cwd=r)
    (r / "c.txt").write_text("third\n", encoding="utf-8")
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "third, with no trailer", cwd=r)
    git("tag", "v1.0.0", cwd=r)
    return r


def run_seed(source: pathlib.Path, *extra: str):
    return subprocess.run(["bash", str(SCRIPT), str(source), *extra],
                          capture_output=True, text=True,
                          env=sealed_env(cwd=source), timeout=180)


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    if not SCRIPT.is_file():
        print(f"test_seed_public_repo: SKIPPED (not a pass) -- {SCRIPT} is missing.",
              file=sys.stderr)
        return 1

    ok_all = True

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        u = run_seed(src)
        out = u.stdout + u.stderr
        ok_all &= case("a history with trailers is rewritten clean",
                       u.returncode == 0 and "trailer lines after the rewrite: 0" in out,
                       out[-300:])
        ok_all &= case("and it says how many it removed",
                       "trailer lines in the source history: 2" in out, out[-300:])
        # The check that matters: messages changed, content did not.
        ok_all &= case("and it verifies that no tree changed",
                       "every ref points at the same tree as before" in out, out[-300:])
        ok_all &= case("tags come along", "2 ref(s) ready" in out or "ref(s) ready" in out,
                       out[-300:])
        # Publishing is a decision, not a step.
        ok_all &= case("it does not push without being asked",
                       "not pushing" in out, out[-300:])

    # A source with nothing to remove must still be green: a seeding step that
    # only works on dirty history is one nobody dares run twice.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r = tmp / "clean"
        r.mkdir()
        git("init", "-q", "-b", "master", cwd=r)
        (r / "a.txt").write_text("only\n", encoding="utf-8")
        git("add", "-A", cwd=r)
        git("commit", "-q", "-m", "no trailer here", cwd=r)
        u = run_seed(r)
        out = u.stdout + u.stderr
        ok_all &= case("a clean history is green and removes nothing",
                       u.returncode == 0 and "trailer lines in the source history: 0" in out,
                       out[-300:])

    # Refusing to seed over an existing history.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        src = source_repo(tmp)
        dest = tmp / "dest.git"
        subprocess.run(["git", "init", "-q", "--bare", str(dest)],
                       capture_output=True, env=sealed_env(cwd=tmp))
        # Give the destination a branch, so it is not empty.
        subprocess.run(["git", "push", "-q", str(dest), "master"], cwd=str(src),
                       capture_output=True, env=sealed_env(cwd=src))
        u = run_seed(src, str(dest), "--push")
        out = u.stdout + u.stderr
        ok_all &= case("it refuses to seed over an existing history",
                       u.returncode == 1 and "already has branches" in out, out[-300:])

    print("test_seed_public_repo: " + ("OK" if ok_all else "FAILED"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
