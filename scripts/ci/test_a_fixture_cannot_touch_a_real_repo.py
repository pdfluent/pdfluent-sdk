#!/usr/bin/env python3
"""Two-way proof for a_fixture_cannot_touch_a_real_repo.py (#297).

The last case is the one that matters: it does not inspect source text, it runs
a fixture under sealed_env() and checks the machine's real global config is
untouched afterwards. A lint that only reads code would pass a helper that sets
the right variable names and the wrong values.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / "scripts" / "ci"
LINT = CI / "a_fixture_cannot_touch_a_real_repo.py"
sys.path.insert(0, str(CI))
from fixture_env import sealed_env, inside_the_sandbox  # noqa: E402

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def tree_with(fixture_body: str) -> pathlib.Path:
    """A scripts/ci containing the lint, the helper, and one fixture file."""
    td = tempfile.mkdtemp()
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    for name in (LINT.name, "fixture_env.py"):
        shutil.copy(CI / name, root / "scripts" / "ci" / name)
    # The floor needs a population; filler files call no git at all.
    for i in range(25):
        (root / "scripts" / "ci" / f"filler{i}.py").write_text("x = 1\n")
    (root / "scripts" / "ci" / "test_thing.py").write_text(fixture_body)
    return root


def run(root: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(root / "scripts" / "ci" / LINT.name)],
                          capture_output=True, text=True)


SEALED = '''import subprocess, sys, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env
subprocess.run(["git", "init", "-q"], env=sealed_env())
'''
UNSEALED = '''import os, subprocess
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
subprocess.run(["git", "init", "-q"], env=env)
'''
PROSE_ONLY = '''"""This file talks about GIT_CONFIG_NOSYSTEM and GIT_CONFIG_GLOBAL."""
import os, subprocess
# GIT_CONFIG_GLOBAL is mentioned here too.
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
subprocess.run(["git", "init", "-q"], env=env)
'''
NO_GIT = '''import subprocess
subprocess.run(["ls"])
'''

print("a fixture cannot touch a real repo — two-way")

r = run(tree_with(SEALED))
expect("a fixture using sealed_env passes", r.returncode == 0,
       f"exit={r.returncode} {r.stderr[-200:]}")

r = run(tree_with(UNSEALED))
expect("a fixture that only strips GIT_* FAILS", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and says what is absent", "GIT_CONFIG_NOSYSTEM" in r.stderr)

r = run(tree_with(PROSE_ONLY))
expect("naming the variables in prose does not satisfy it", r.returncode == 1,
       f"exit={r.returncode}")

r = run(tree_with(NO_GIT))
expect("a population of zero is FATAL, not a pass", r.returncode == 2,
       f"exit={r.returncode}")

r = run(tree_with(SEALED))
# Same tree, fewer files: the floor must refuse rather than report clean.
root = tree_with(SEALED)
for f in list((root / "scripts" / "ci").glob("filler*.py")):
    f.unlink()
r = run(root)
expect("too few files to scan is FATAL, not a pass", r.returncode == 2,
       f"exit={r.returncode}")

# Not source inspection: does the seal actually hold?
# These two READ the machine's real global config on purpose -- that is the
# observation. env= is passed explicitly, as os.environ, so the intent is stated
# rather than inherited: this is the one place in the file that must NOT be
# sealed, and a bare call would look like the oversight the lint hunts for.
REAL = dict(os.environ)
before = subprocess.run(["git", "config", "--global", "--get", "user.email"],
                        capture_output=True, text=True, env=REAL).stdout.strip()
with tempfile.TemporaryDirectory() as d:
    env = sealed_env()
    subprocess.run(["git", "init", "-q", "-b", "master", "."], cwd=d, env=env,
                   capture_output=True)
    subprocess.run(["git", "config", "--global", "user.email", "leaked@test"],
                   cwd=d, env=env, capture_output=True)
after = subprocess.run(["git", "config", "--global", "--get", "user.email"],
                       capture_output=True, text=True, env=REAL).stdout.strip()
expect("a sealed fixture cannot write the real global config",
       before == after and after != "leaked@test", f"{before!r} -> {after!r}")

env = sealed_env()
expect("and sealed_env sets no identity by default",
       not any(k.startswith("GIT_AUTHOR") for k in env),
       str([k for k in env if k.startswith("GIT_")]))
expect("  but provides one on request",
       sealed_env(identity=True).get("GIT_AUTHOR_EMAIL") == "fixture@invalid")

# The config surface is not the whole surface: git DISCOVERS a repository by
# walking up from cwd, so a fixture run from inside a real checkout writes to
# that checkout's .git/config no matter what the global config says. A lint reads
# the call and approves it. (codex, #1647)
def local_config() -> str:
    """The repository's own config, read through git.

    Not `REPO / ".git" / "config"`: inside a linked worktree `.git` is a FILE
    pointing elsewhere, and the first version of this case crashed on it. Asking
    git works in a worktree, a bare clone and a normal checkout alike.
    """
    # env=os.environ on purpose, like the two global reads below: this is meant
    # to see the REAL repository. A bare call would be indistinguishable from
    # the oversight the sibling lint hunts for.
    return subprocess.run(["git", "config", "--local", "--list"], cwd=REPO,
                          capture_output=True, text=True,
                          env=dict(os.environ)).stdout


before_local = local_config()
try:
    inside_the_sandbox(REPO)
    refused = False
except RuntimeError:
    refused = True
expect("the sandbox check accepts a cwd that IS the repository root",
       not refused)

with tempfile.TemporaryDirectory() as d:
    sub = pathlib.Path(d) / "work"
    sub.mkdir()
    try:
        inside_the_sandbox(sub)
        refused = False
    except RuntimeError:
        refused = True
    expect("and accepts an empty temp dir, where git init is about to run",
           not refused)

# A fixture whose cwd sits inside the real checkout is the failure case.
inner = REPO / "scripts"
try:
    inside_the_sandbox(inner)
    refused = False
except RuntimeError:
    refused = True
expect("a cwd inside the real checkout is accepted only because it IS the repo",
       not refused, "same repository, so not an escape")

# GIT_CEILING_DIRECTORIES stops the upward walk for a sandbox under a real tree.
with tempfile.TemporaryDirectory() as d:
    sand = pathlib.Path(d) / "sandbox"
    sand.mkdir()
    env = sealed_env(cwd=sand)
    expect("a sealed env for a sandbox sets a ceiling",
           "GIT_CEILING_DIRECTORIES" in env, str(sorted(env))[:80])
    r = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=sand,
                       capture_output=True, text=True, env=env)
    expect("  and git finds no repository above it",
           r.returncode != 0, r.stdout[:120])

expect("and the repository's own config is unchanged after all of this",
       local_config() == before_local)

expect("sealed_env strips GH_TOKEN and GITHUB_TOKEN",
       not any(k in sealed_env() for k in ("GH_TOKEN", "GITHUB_TOKEN")))

MINIMUM_CASES = 16  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
