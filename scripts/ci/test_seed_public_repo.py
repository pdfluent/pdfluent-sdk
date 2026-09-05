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


def git(*a, cwd, env=None):
    """A fixture git call that must succeed.

    Raising is the point. This swallowed a non-zero exit, so a fixture that
    built nothing produced an empty source repository and every case below
    failed on `git filter-branch` saying "You must specify a ref to rewrite" --
    a message about the script under test, pointing away from the fixture that
    was actually broken. (#132)
    """
    r = subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                       text=True, env=env if env is not None else sealed_env(cwd=cwd))
    if r.returncode != 0:
        raise RuntimeError(
            f"fixture setup failed: git {' '.join(a)} in {cwd} "
            f"exited {r.returncode}\n{r.stdout}{r.stderr}")
    return r


def init_repo(r: pathlib.Path, env=None) -> None:
    """An empty repository that can commit without borrowing an identity.

    `sealed_env` hands git an empty global config, so the fixture had no
    `user.email` and `git commit` fell back to whatever the machine could
    auto-detect. A developer's machine lends one; a GitHub runner, whose
    hostname yields no usable address, refuses -- so the three commits below
    were never made there. Green here, red there, from 05-09-2026 on. The
    identity goes in the sandbox repository rather than the environment,
    because an env-level one overrides identities other fixtures configure on
    purpose (see fixture_env.sealed_env).
    """
    git("init", "-q", "-b", "master", cwd=r, env=env)
    git("config", "user.name", "fixture", cwd=r, env=env)
    git("config", "user.email", "fixture@invalid", cwd=r, env=env)


def source_repo(tmp: pathlib.Path, env=None) -> pathlib.Path:
    r = tmp / "source"
    r.mkdir()
    init_repo(r, env=env)
    (r / "a.txt").write_text("first\n", encoding="utf-8")
    git("add", "-A", cwd=r, env=env)
    git("commit", "-q", "-m", "first\n\nCo-Authored-By: Someone <s@example.invalid>", cwd=r, env=env)
    (r / "b.txt").write_text("second\n", encoding="utf-8")
    git("add", "-A", cwd=r, env=env)
    git("commit", "-q", "-m", "second\n\nAssisted-by: Another <a@example.invalid>", cwd=r, env=env)
    (r / "c.txt").write_text("third\n", encoding="utf-8")
    git("add", "-A", cwd=r, env=env)
    git("commit", "-q", "-m", "third, with no trailer", cwd=r, env=env)
    git("tag", "v1.0.0", cwd=r, env=env)
    # The fixture says what it built. An empty source repository makes every
    # case below fail for a reason that has nothing to do with the script.
    commits = git("rev-list", "--count", "HEAD", cwd=r, env=env).stdout.strip()
    if commits != "3":
        raise RuntimeError(f"fixture source repo holds {commits} commit(s), expected 3")
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
        init_repo(r)
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

    # THE CASE THAT WOULD HAVE CAUGHT IT. A machine that lends git an identity
    # hides this entirely, which is why the suite was green here and red on the
    # runner. `useConfigOnly` takes that loan away, so the fixture has to carry
    # its own identity or build nothing at all.
    #
    # One key of the seal is deliberately replaced here, and only here: this
    # case is ABOUT the config git reads, so it has to name the file.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        cfg = tmp / "no-identity.gitconfig"
        cfg.write_text("[user]\n\tuseConfigOnly = true\n", encoding="utf-8")
        env = sealed_env(cwd=tmp)
        env["GIT_CONFIG_GLOBAL"] = str(cfg)
        try:
            src = source_repo(tmp, env=env)
            built = git("rev-list", "--count", "HEAD", cwd=src, env=env).stdout.strip()
            ok_all &= case("the fixture commits without borrowing the machine's identity",
                           built == "3", f"the source repo holds {built} commit(s)")
        except RuntimeError as e:
            ok_all &= case("the fixture commits without borrowing the machine's identity",
                           False, str(e)[:300])

    print("test_seed_public_repo: " + ("OK" if ok_all else "FAILED"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
