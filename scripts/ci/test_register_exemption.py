#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the shared-register exemption opens, and what it must keep shut.

Case 2 is the one that decides whether this is safe: editing a shared register
WITHOUT adding the file the new row names has to stay refused. Otherwise the
exemption is not "register what you are adding" but "edit t3's files from
anywhere", which is a different rule nobody agreed to.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

GUARD = CI / "territories_do_not_overlap.py"

MAP = '''[[territory]]
id = "t1"
name = "Engine"
paden = ["crates/**", "fixtures/**"]

[[territory]]
id = "t3"
name = "Licensing"
paden = ["scripts/ci/corpus_herkomst.py", "docs/**"]

[registers]
gedeeld = ["scripts/ci/corpus_herkomst.py"]
'''


def git(*a, cwd):
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                          text=True, env=sealed_env(cwd=cwd))


def repo(tmp: pathlib.Path) -> pathlib.Path:
    r = tmp / "repo"
    (r / ".claude").mkdir(parents=True)
    (r / "scripts" / "ci").mkdir(parents=True)
    (r / "crates").mkdir()
    (r / "fixtures").mkdir()
    (r / "docs").mkdir()
    shutil.copy(GUARD, r / "scripts" / "ci" / GUARD.name)
    (r / ".claude" / "territories.toml").write_text(MAP, encoding="utf-8")
    (r / "scripts" / "ci" / "corpus_herkomst.py").write_text(
        "EIGEN_LOS = {\n}\n", encoding="utf-8")
    (r / "crates" / "a.rs").write_text("pub fn a() {}\n", encoding="utf-8")
    git("init", "-q", "-b", "master", cwd=r)
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "base", cwd=r)
    kaal = tmp / "bare.git"
    subprocess.run(["git", "init", "-q", "--bare", str(kaal)],
                   capture_output=True, env=sealed_env(cwd=tmp))
    git("remote", "add", "origin", str(kaal), cwd=r)
    git("push", "-q", "origin", "master", cwd=r)
    git("checkout", "-q", "-b", "t1/work", cwd=r)
    return r


def run_guard(r: pathlib.Path):
    return subprocess.run([sys.executable, str(r / "scripts" / "ci" / GUARD.name)],
                          cwd=str(r), capture_output=True, text=True,
                          env=sealed_env(cwd=r))


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    ok_all = True

    # 1. Register the file you are adding, from another territory: allowed.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d))
        (r / "fixtures" / "new.pdf").write_bytes(b"%PDF-1.4\n")
        p = r / "scripts" / "ci" / "corpus_herkomst.py"
        p.write_text('EIGEN_LOS = {\n    "fixtures/new.pdf": "handmade",\n}\n',
                     encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "add and register", cwd=r)
        u = run_guard(r)
        ok_all &= case("a t1 branch may register the fixture it adds",
                       u.returncode == 0, (u.stdout + u.stderr)[:250])

    # 2. THE case: edit the register without adding what it names -- refused.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d))
        p = r / "scripts" / "ci" / "corpus_herkomst.py"
        p.write_text('EIGEN_LOS = {\n    "fixtures/elsewhere.pdf": "not added here",\n}\n',
                     encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "register only", cwd=r)
        u = run_guard(r)
        ok_all &= case("editing the register without adding the file is refused",
                       u.returncode == 1 and "corpus_herkomst.py" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    # 3. Adding a file and editing the register for a DIFFERENT one: refused.
    #    Without this the exemption would be "add anything, then edit freely".
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d))
        (r / "fixtures" / "new.pdf").write_bytes(b"%PDF-1.4\n")
        p = r / "scripts" / "ci" / "corpus_herkomst.py"
        p.write_text('EIGEN_LOS = {\n    "fixtures/other.pdf": "a different file",\n}\n',
                     encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "mismatched", cwd=r)
        u = run_guard(r)
        ok_all &= case("registering a file other than the one added is refused",
                       u.returncode == 1, (u.stdout + u.stderr)[:250])

    # 4. A file that is not declared shared stays closed.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d))
        (r / "fixtures" / "new.pdf").write_bytes(b"%PDF-1.4\n")
        (r / "docs" / "something.md").write_text("fixtures/new.pdf\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "docs edit", cwd=r)
        u = run_guard(r)
        ok_all &= case("a t3 file that is not a declared register stays closed",
                       u.returncode == 1 and "something.md" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    print("test_register_exemption: " + ("OK" if ok_all else "FAILED"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
