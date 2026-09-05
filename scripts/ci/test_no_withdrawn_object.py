#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The guard from #260, and above all the ways it must not work.

Case 1 is the assignment: not only the tip of the push. Case 2 is why it looks at
the object and not the name. Case 5 is why it exists without naming the SHA.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

GUARD = CI / "no_withdrawn_object.py"
CONTENT = b"the withdrawn document, a few bytes in this test\n"


def git(*a, cwd):
    """A fixture git call that must succeed.

    Raising, and the identity, are one fix. `sealed_env` hands git an empty
    global config, so `git commit` had no `user.email` and fell back to what
    the machine could auto-detect. A developer's machine lends one; a GitHub
    runner, whose hostname yields no usable address, refuses. The fixture's
    commits were therefore never made there and the guard was handed a
    repository with no HEAD -- which it reported as its own failure. Swallowing
    the non-zero exit is what let that read as a finding about the subject
    instead of about the fixture. (#331, #132)
    """
    r = subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                       text=True, env=sealed_env(cwd=cwd))
    if r.returncode != 0:
        raise RuntimeError(
            f"fixture setup failed: git {' '.join(a)} in {cwd} "
            f"exited {r.returncode}\n{r.stdout}{r.stderr}")
    return r


def build(tmp: pathlib.Path) -> tuple[pathlib.Path, str]:
    """A repository with master, and the blob SHA of the forbidden content."""
    r = tmp / "repo"
    (r / "scripts" / "ci").mkdir(parents=True)
    shutil.copy(GUARD, r / "scripts" / "ci" / GUARD.name)
    (r / "leesmij.md").write_text("base\n", encoding="utf-8")
    git("init", "-q", "-b", "master", cwd=r)
    # The identity lives in the sandbox repository, not the environment: an
    # env-level one overrides identities other fixtures configure on purpose
    # (see fixture_env.sealed_env).
    git("config", "user.name", "fixture", cwd=r)
    git("config", "user.email", "fixture@invalid", cwd=r)
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "base", cwd=r)
    # A real bare remote, the way the sign-off fixture does it too. Without
    # `origin/master` the guard falls back to "the whole branch", and then you
    # are testing the fallback instead of the range a push actually adds.
    bare = tmp / "bare.git"
    subprocess.run(["git", "init", "-q", "--bare", str(bare)], capture_output=True,
                   env=sealed_env(cwd=tmp))
    git("remote", "add", "origin", str(bare), cwd=r)
    git("push", "-q", "origin", "master", cwd=r)
    sha = subprocess.run(["git", "hash-object", "-w", "--stdin"], cwd=str(r),
                         input=CONTENT, capture_output=True,
                         env=sealed_env(cwd=r)).stdout.decode().strip()
    return r, sha


def run_guard(r: pathlib.Path, blocked: pathlib.Path | None):
    env = sealed_env(cwd=r)
    env["PDFLUENT_WITHDRAWN_OBJECTS"] = str(blocked) if blocked else str(r / "does-not-exist")
    return subprocess.run([sys.executable, "scripts/ci/no_withdrawn_object.py"],
                          cwd=str(r), capture_output=True, text=True, env=env)


def case(name: str, ok: bool, waarom: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FOUT'}  {name}")
    if not ok and waarom:
        print(f"        {waarom}")
    return ok


def main() -> int:
    ok = True

    # 1. The blob arrives in a commit that is NOT the tip, with two innocent
    #    commits on top of it. A guard that looks only at the tip -- or only at
    #    the working tree -- sees nothing here.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = build(tmp)
        blocked = tmp / "blocklist.txt"
        blocked.write_text(f"# a comment line\n{sha}\n", encoding="utf-8")
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "formulier.pdf").write_bytes(CONTENT)
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "voegt toe", cwd=r)
        (r / "formulier.pdf").unlink()
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "haalt weer weg", cwd=r)
        (r / "iets.md").write_text("onschuldig\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "iets anders", cwd=r)
        u = run_guard(r, blocked)
        ok &= case("a blob in a commit that is not the tip is caught",
                      u.returncode == 1 and "formulier.pdf" in u.stderr,
                      (u.stdout + u.stderr)[:250])
        # 5. A hit must not publish the thing it guards.
        ok &= case("a hit does not name the SHA",
                      sha not in (u.stdout + u.stderr),
                      "the SHA appears in the output")

    # 2. The same content under a different name. This is the difference from a
    #    name-based guard, and the reason #260 asks for this one.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = build(tmp)
        blocked = tmp / "blocklist.txt"; blocked.write_text(sha + "\n", encoding="utf-8")
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "docs").mkdir()
        (r / "docs" / "a-completely-different-name.pdf").write_bytes(CONTENT)
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "hernoemd", cwd=r)
        u = run_guard(r, blocked)
        ok &= case("renaming does not help: the object is what counts",
                      u.returncode == 1 and "a-completely-different-name.pdf" in u.stderr,
                      (u.stdout + u.stderr)[:250])

    # 3. What is already on master is not what this push adds.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = build(tmp)
        blocked = tmp / "blocklist.txt"; blocked.write_text(sha + "\n", encoding="utf-8")
        (r / "al-aanwezig.pdf").write_bytes(CONTENT)
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "stond er al", cwd=r)
        git("push", "-q", "origin", "master", cwd=r)
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "nieuw.md").write_text("niets bijzonders\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "gewoon werk", cwd=r)
        u = run_guard(r, blocked)
        ok &= case("what was already on master is not a finding of this push",
                      u.returncode == 0, (u.stdout + u.stderr)[:250])

    # 4. No list is not green.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, _ = build(tmp)
        u = run_guard(r, None)
        ok &= case("without the list: SKIPPED (not a pass), not green",
                      u.returncode != 0 and "SKIPPED (not a pass)" in (u.stdout + u.stderr),
                      (u.stdout + u.stderr)[:250])

    # 6. A clean branch is green, and says over what.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = build(tmp)
        blocked = tmp / "blocklist.txt"; blocked.write_text(sha + "\n", encoding="utf-8")
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "gewoon.md").write_text("werk\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "werk", cwd=r)
        u = run_guard(r, blocked)
        ok &= case("a clean branch is green and names what it looked at",
                      u.returncode == 0 and "object(s) from" in u.stdout
                      and "commit(s)" in u.stdout,
                      (u.stdout + u.stderr)[:250])

    print("test_no_withdrawn_object: " + ("OK" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
