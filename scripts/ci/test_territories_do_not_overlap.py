#!/usr/bin/env python3
"""The territory guard, exercised on repositories built for the purpose (#1636).

Two failures it had, both of the same kind: a verdict decided in one arm and a
verdict decided in another, with no place that saw both.

  1. Three arms each returned their own exit code. An overlap in the map --
     found, collected, and worth exit 1 -- was reported only if the arm that
     happened to run bothered to print it. The detached-HEAD arm was fixed to
     print it; the unnamed-branch arm still swallowed it, and a detached HEAD
     carrying only `feature/alias` lands in exactly that arm.
  2. `current_branch()` returned the first territory ref git listed when
     several pointed at the same commit, so listing order chose the owner.

Fixtures are sealed with fixture_env.sealed_env: a test that builds a git repo
and picks up the developer's own config is measuring the developer. (#1647)
"""
from __future__ import annotations
import pathlib, shutil, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402

GUARD = "scripts/ci/territories_do_not_overlap.py"
GIT = "/usr/bin/git"

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


OVERLAPPING = """
[[territory]]
id = "t1"
paden = ["scripts/shared/**"]

[[territory]]
id = "t2"
paden = ["scripts/shared/**"]
"""

CLEAN = """
[[territory]]
id = "t1"
paden = ["scripts/t1/**"]

[[territory]]
id = "t2"
paden = ["scripts/ci/**"]
"""


_WEGWERP: list = []


def build(map_text: str, branches: list[str], detach: bool,
          commit_na_detach: bool = False):
    """A repo with a master, one commit on top, and the branches asked for.

    `commit_na_detach` makes one more commit AFTER detaching, so HEAD is a
    commit no branch points at. Without it the "detached and nameless" case
    cannot be built at all: this builder commits and then detaches, so `master`
    still points at HEAD and `--points-at` always finds it. That is why the two
    mutations in the SKIPPED path survived -- there was no fixture that could
    reach it. (peer review, #1636)
    """
    # Held, not leaked: mkdtemp does not clean up, and this builder is called
    # a dozen times a run. Third time today this leak has come back, twice in
    # suites I wrote after measuring it.
    _td = tempfile.TemporaryDirectory()
    _WEGWERP.append(_td)
    td = _td.name
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / ".claude").mkdir()
    shutil.copy(REPO / GUARD, root / GUARD)
    shutil.copy(REPO / "scripts" / "ci" / "fixture_env.py",
                root / "scripts" / "ci" / "fixture_env.py")
    env = sealed_env(identity=True, cwd=root)

    def git(*a, check=True):
        r = subprocess.run([GIT, *a], cwd=root, env=env, capture_output=True, text=True)
        assert not check or r.returncode == 0, f"git {' '.join(a)}: {r.stderr}"
        return r

    git("init", "-q", "-b", "master")
    (root / ".claude" / "territories.toml").write_text(CLEAN)
    git("add", "-A"); git("commit", "-qm", "base")
    # `github/master` is what the guard diffs against, so the fixture provides
    # one rather than a remote it would have to reach over the network.
    git("update-ref", "refs/remotes/github/master", "HEAD")
    (root / ".claude" / "territories.toml").write_text(map_text)
    (root / "scripts" / "ci" / "touched.py").write_text("# owned by t2\n")
    git("add", "-A"); git("commit", "-qm", "work")
    for b in branches:
        git("branch", b)
    if detach:
        git("checkout", "-q", "--detach", "HEAD")
        if commit_na_detach:
            (root / "scripts" / "ci" / "los.py").write_text("# no branch points here\n")
            git("add", "-A"); git("commit", "-qm", "detached work")
    return root, env


def run(root, env, **extra):
    return subprocess.run([sys.executable, str(root / GUARD)], cwd=root,
                          env=dict(env, **extra), capture_output=True, text=True)


print("the territory guard — one judgement on everything collected")

# 1. The exact shape Codex named: a detached HEAD whose only ref names no
#    territory, over a map that contradicts itself.
root, env = build(OVERLAPPING, ["feature/alias"], detach=True)
r = run(root, env)
expect("an overlap is reported from the unnamed-branch arm", r.returncode == 1,
       f"exit={r.returncode}: {r.stderr[:160]}")
expect("  and it names both claimants", "t1" in r.stderr and "t2" in r.stderr)
expect("  and still says the branch half did not run", "feature/alias" in r.stderr
       or "no territory" in r.stderr, r.stderr[:160])

# 2. The same map, on a branch that does name a territory: still exit 1.
root, env = build(OVERLAPPING, [], detach=False)
r = run(root, env, TERRITORY_BRANCH="t2/work")
expect("an overlap is reported from the territory arm", r.returncode == 1,
       f"exit={r.returncode}")

# 3. A clean map on an unnamed branch that touches owned ground is still a
#    failure -- opting out by renaming stays closed.
root, env = build(CLEAN, ["feature/alias"], detach=True)
r = run(root, env)
expect("an unnamed branch touching owned files fails", r.returncode == 1,
       f"exit={r.returncode}: {r.stderr[:160]}")
expect("  and names the file it reached", "touched.py" in r.stderr)

# 4. Ambiguity is raised, not resolved by listing order.
root, env = build(CLEAN, ["t1/one", "t2/two"], detach=True)
r = run(root, env)
expect("two territory refs on one commit is an error", r.returncode == 2,
       f"exit={r.returncode}: {r.stderr[:160]}")
expect("  and both names are in the message",
       "t1/one" in r.stderr and "t2/two" in r.stderr, r.stderr[:200])

# 5. One territory ref is still resolved.
root, env = build(CLEAN, ["t2/only"], detach=True)
r = run(root, env)
expect("a single territory ref resolves", r.returncode == 0,
       f"exit={r.returncode}: {r.stderr[:160]}")

# 6. TERRITORY_BRANCH wins over refs, so ambiguity has a stated way out.
root, env = build(CLEAN, ["t1/one", "t2/two"], detach=True)
r = run(root, env, TERRITORY_BRANCH="t2/two")
expect("TERRITORY_BRANCH resolves an ambiguous checkout", r.returncode == 0,
       f"exit={r.returncode}: {r.stderr[:160]}")

# The map is read AS COMMITTED. Claiming used to be free: touch a path
# somebody else owns, add the claim to the map, leave it uncommitted, pass.
root, env = build(CLEAN, [], detach=False)
(root / ".claude" / "territories.toml").write_text(CLEAN.replace(
    'paden = ["scripts/ci/**"]', 'paden = ["scripts/ci/**", "scripts/t1/**"]'))
subprocess.run([GIT, "checkout", "-qb", "t1/reaching"], cwd=root, env=env, check=True)
(root / "scripts" / "ci" / "not-mine.py").write_text("# t2 owns scripts/ci\n")
subprocess.run([GIT, "add", "scripts/ci/not-mine.py"], cwd=root, env=env, check=True)
subprocess.run([GIT, "commit", "-qm", "reach"], cwd=root, env=env, check=True)
r = run(root, env, TERRITORY_BRANCH="t1/reaching")
expect("an UNCOMMITTED claim does not excuse the reach", r.returncode == 1,
       f"exit={r.returncode}: {r.stderr[:200]}")

# Two branches of one territory are two names, not two owners.
root, env = build(CLEAN, ["t2/one", "t2/two"], detach=True)
r = run(root, env)
expect("two branches of the SAME territory are not a conflict", r.returncode == 0,
       f"exit={r.returncode}: {r.stderr[:160]}")

# The CI shape: detached at a commit whose only ref is remote-tracking.
root, env = build(CLEAN, [], detach=False)
subprocess.run([GIT, "update-ref", "refs/remotes/github/t2/from-ci", "HEAD"],
               cwd=root, env=env, check=True)
subprocess.run([GIT, "checkout", "-q", "--detach", "HEAD"], cwd=root, env=env, check=True)
r = run(root, env)
expect("a remote-tracking ref names the territory", r.returncode == 0,
       f"exit={r.returncode}: {r.stderr[:200]}")

# THE HOME, same assertion as the hook guard's: a guard nothing runs is a
# comment, and `every_guard_has_a_job` stayed green when the gate line went.
#
# Through the shared helper, matching the exact script ARGUMENT. Written as a
# substring first -- `"territories_do_not_overlap.py" in poort` -- which the
# line running THIS file satisfies, so deleting the guard's own invocation left
# it green. The assertion that a check still had a home was kept alive by the
# check itself. Measured on the hook guard's twin, #1660. (codex, #1660)
#
# Local rather than shared on purpose: the shared `fixture_env.gate_aanroepen`
# lands with #1660, and importing it here would make this branch depend on a
# merge order. It collapses into that helper once #1660 is in master.
def gate_aanroepen(script: str) -> list[str]:
    poort = (REPO / "scripts" / "ci" / "local_ci_gate.sh").read_text(errors="replace")
    return [r for r in poort.splitlines()
            if any(x.rsplit("/", 1)[-1] == script for x in r.split())]


expect("the local gate still runs the guard itself",
       len(gate_aanroepen("territories_do_not_overlap.py")) == 1,
       str(gate_aanroepen("territories_do_not_overlap.py")))
expect("  and its test is wired as its own line",
       len(gate_aanroepen("test_territories_do_not_overlap.py")) == 1)

# Both at once: an overlapping map AND a checkout whose owner cannot be
# decided. The overlap must survive -- ambiguity is a reason the branch half
# cannot run, not a reason to stop reporting the half that did.
root, env = build(OVERLAPPING, ["t1/one", "t2/two"], detach=True)
r = run(root, env)
expect("an overlap survives an ambiguous checkout", r.returncode == 1,
       f"exit={r.returncode}: {r.stderr[:200]}")
expect("  and the ambiguity is still reported",
       "t1/one" in r.stderr and "t2/two" in r.stderr, r.stderr[:250])
expect("  and the overlap is named", "both claim" in r.stderr, r.stderr[:250])

# `feature/t2/disguised` is a LOCAL branch, not a remote-tracking ref. Stripping
# any leading segment that was not a territory read `feature` as a remote and
# handed the branch t2's territory -- a branch could take on an owner it never
# claimed by being named after nothing in particular. (codex, #1636)
root, env = build(CLEAN, ["feature/t2/disguised"], detach=True)
r = run(root, env)
expect("a branch named feature/<territory>/x does not become that territory",
       r.returncode == 1, f"exit={r.returncode}: {r.stderr[:200]}")
expect("  and it is judged as naming no territory",
       "no territory" in r.stderr, r.stderr[:250])

# THE HEADLINE CASE OF THIS PULL REQUEST, and until now untested: detached,
# no ref pointing here, nothing in the environment. Two mutations survived the
# whole suite because of it -- turning the SKIPPED exit into 0, and deleting
# the "SKIPPED (not a pass)" line itself -- which means the half of the guard
# this PR is named after was never exercised.
root, env = build(CLEAN, [], detach=True, commit_na_detach=True)
schoon = {k: v for k, v in env.items()
          if k not in ("GITHUB_HEAD_REF", "GITHUB_REF_NAME", "CI_COMMIT_REF_NAME",
                       "TERRITORY_BRANCH")}
r = subprocess.run([sys.executable, str(root / GUARD)], cwd=root,
                   env=schoon, capture_output=True, text=True)
expect("a nameless detached HEAD is SKIPPED, not a pass", r.returncode == 3,
       f"exit={r.returncode}: {r.stderr[:200]}")
expect("  and it says so in those words",
       "SKIPPED (not a pass)" in r.stderr, r.stderr[:250])
expect("  and states the map itself was checked",
       "map" in r.stderr.lower(), r.stderr[:250])

# THE VARIABLE THE GUARD RECOMMENDS. `TERRITORY_BRANCH` is what the SKIPPED
# message tells a detached HEAD to set, so it is set precisely when somebody is
# pushing -- and the fixtures inherited it, which made every nameless branch
# they built look like it was called that. Ten cases failed with it set,
# including three asserting an UNNAMED branch is treated as unnamed. The seal
# strips it now.
#
# Which could just as easily disable the documented escape, so this proves it
# still works when a caller means it: passed deliberately, on a detached HEAD,
# it names the territory.
root, env = build(CLEAN, [], detach=True, commit_na_detach=True)
# t1, deliberately: the fixture's change is in scripts/ci, which CLEAN gives to
# t2. A t2 name would pass and prove nothing -- it has to be a name that makes
# the same change a violation, or the assertion cannot tell "the variable was
# read" from "the variable was ignored and nothing was wrong anyway".
r = run(root, env, TERRITORY_BRANCH="t1/deliberate")
expect("a deliberately passed TERRITORY_BRANCH is still read",
       r.returncode == 1 and "t1/deliberate" in r.stderr,
       f"exit={r.returncode}: {r.stderr[:200]}")
expect("  and it judges the branch rather than skipping it",
       "SKIPPED" not in r.stderr, r.stderr[:200])

MINIMUM_CASES = 25  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
