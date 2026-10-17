#!/usr/bin/env python3
"""A test that builds a git repository must not be able to reach a real one (#297).

test_no_test_can_touch_the_real_repo.py checks that every git call passes `env=`.
That stops GIT_DIR from redirecting a fixture at the real repository. It does not
stop the other half, and both halves happened on the same night in two terminals:

  GIT_DIR inherited from a pre-push hook, so `init` and `commit` ran against the
  real repository. Recovered from the reflog.

  `git config user.email t@t` written by a fixture into a real worktree's local
  config, after which every rebase there stamped a test identity onto commits.

Dropping GIT_* fixes the first. The second needs the config surface sealed as
well: GIT_CONFIG_NOSYSTEM so /etc/gitconfig cannot answer, and GIT_CONFIG_GLOBAL
pointed at an empty file so the user's ~/.gitconfig neither leaks in nor is
written to. scripts/ci/fixture_env.py does all three.

WHAT THIS CHECKS, AND WHAT IT DOES NOT

Only files that BUILD or CONFIGURE a repository -- ones calling git with "init"
or "config". Those are the fixtures; a read-only guard querying the repository it
stands in is a different thing and is covered by the other lint.

It looks for the sealing keys as real string constants in the code, not as text:
docstrings and comments are excluded, because a file explaining that it should
seal its config is not a file that seals its config. That distinction is not
theoretical -- an assertion of mine matched its own comment about the thing it
forbade, earlier the same night.

It does not prove the keys reach the subprocess. A file can import fixture_env
and forget one call. That is what test_no_test_can_touch_the_real_repo.py's
per-call `env=` rule covers, and what the fixtures' own runs demonstrate.
"""
from __future__ import annotations
import ast
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
MAP = REPO / "scripts" / "ci"

REQUIRED = ("GIT_CONFIG_NOSYSTEM", "GIT_CONFIG_GLOBAL")
# "init" or "config" -- a fixture BUILDS a repository. Deliberately not "clone":
# the register guards clone real upstream repositories into a cache, which is a
# different act with a different owner, and sweeping them in here would have this
# lint rewrite someone else's file as a side effect of its own scope.
BUILDS_A_REPO = ("init", "config")

# Files that legitimately read the real repository's config rather than building
# a fixture. Each needs a reason, and the reason is checked by a human, not here.
ALLOWED: dict[str, str] = {
    "de_gedeelde_config_breekt_geen_worktrees.py": (
        "its subject IS the real repository's shared config; sealing the config "
        "surface would hide the exact thing it inspects"
    ),
    "fixture_env.py": "it is the helper; the keys are what it sets",
    "a_fixture_cannot_touch_a_real_repo.py": "this file; the keys are what it looks for",
    "mirror_has_not_drifted.py": (
        "its subject IS this clone's two remotes: `--fetch` asks git for "
        "`remote.<name>.url` and then refreshes that ref. Sealing the config "
        "surface would point GIT_CONFIG_GLOBAL at an empty file, which removes "
        "the credential helper the fetch needs and turns every run into an "
        "unreachable remote -- the guard would then always warn and never "
        "compare, which is the permanent skip this whole family refuses. It "
        "builds nothing: one `config --get`, one `fetch`, no init and no commit, "
        "and it already drops every GIT_* variable so a hook's GIT_DIR cannot "
        "redirect it (#231)"
    ),
    "every_worktree_runs_the_tracked_hooks.py": (
        "its subject IS this clone's worktrees: it asks each one what git will "
        "use as its hooks directory. A sandbox check would refuse the only "
        "repository it has any reason to look at, and it never writes -- one "
        "`git worktree list` and one `git config --get` per worktree, no init "
        "and no commit (#454)"
    ),
    "the_commit_msg_hook_is_wired.py": (
        "its subject IS this clone: it asks whether core.hooksPath points at "
        ".githooks here. A sandbox check would refuse the only repository it "
        "has any reason to look at, and it never writes -- one `git config "
        "--get`, no init, no commit"
    ),
}

MINIMUM_FILES = 20  # FLOOR


def code_strings(tree: ast.AST) -> set[str]:
    """String constants that are not docstrings, so prose cannot satisfy a rule."""
    docstrings = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef,
                             ast.AsyncFunctionDef)):
            body = getattr(node, "body", None)
            if body and isinstance(body[0], ast.Expr) and \
               isinstance(body[0].value, ast.Constant) and \
               isinstance(body[0].value.value, str):
                docstrings.add(id(body[0].value))
    return {n.value for n in ast.walk(tree)
            if isinstance(n, ast.Constant) and isinstance(n.value, str)
            and id(n) not in docstrings}


def calls_made(tree: ast.AST) -> set[str]:
    """Function names that are actually CALLED.

    Separate from imports on purpose: an unused `from fixture_env import
    sealed_env` sitting beside the old environment satisfied a name check while
    changing nothing. An import is an intention; a call is the behaviour.
    (codex, #1647)
    """
    out: set[str] = set()
    for n in ast.walk(tree):
        if isinstance(n, ast.Call):
            f = n.func
            if isinstance(f, ast.Name):
                out.add(f.id)
            elif isinstance(f, ast.Attribute):
                out.add(f.attr)
    return out


def cwd_kwargs(tree: ast.AST) -> set[str]:
    """Keyword names passed to any sealed_env() call in the file."""
    out: set[str] = set()
    for n in ast.walk(tree):
        if isinstance(n, ast.Call):
            f = n.func
            name = f.id if isinstance(f, ast.Name) else getattr(f, "attr", "")
            if name == "sealed_env":
                out.update(k.arg for k in n.keywords if k.arg)
    return out


def names_used(tree: ast.AST) -> set[str]:
    """Imported and called names.

    Separate from code_strings because the two questions are different, and the
    first version of this lint conflated them: it looked for "sealed_env" among
    STRING constants, where an identifier never appears. Every converted file
    was reported as unsealed. A check that cannot recognise the fix is the mirror
    of one that cannot fail -- it also measures nothing, it just says no instead
    of yes.
    """
    out: set[str] = set()
    for n in ast.walk(tree):
        if isinstance(n, ast.ImportFrom) and n.module:
            out.add(n.module)
            out.update(a.name for a in n.names)
        elif isinstance(n, ast.Import):
            out.update(a.name for a in n.names)
        elif isinstance(n, ast.Name):
            out.add(n.id)
        elif isinstance(n, ast.Attribute):
            out.add(n.attr)
    return out


# What a shell fixture has to call, by path, so a rename of the helper is a
# failure here rather than a lint that silently stops asking.
SENTINEL = "scripts/ci/_the_checkout_is_unchanged.sh"
SEAL = "_a_fixture_inherits_no_git_env.sh"


def main() -> int:
    files = sorted(MAP.glob("*.py"))
    if len(files) < MINIMUM_FILES:  # FLOOR
        print(f"[fixture-env] FATAL: only {len(files)} file(s) found in {MAP}; "
              f"the floor is {MINIMUM_FILES}. A scan that looked at almost "
              "nothing must not report a clean result -- run it from the "
              "repository.", file=sys.stderr)
        return 2

    problems: list[str] = []
    checked = 0
    for path in files:
        if path.name in ALLOWED:
            continue
        try:
            tree = ast.parse(path.read_text(encoding="utf-8", errors="replace"))
        except SyntaxError as exc:
            problems.append(f"{path.name}: does not parse ({exc})")
            continue
        strings = code_strings(tree)
        used = calls_made(tree)
        if not any(v in strings for v in BUILDS_A_REPO):
            continue
        checked += 1
        # Calling sealed_env is not enough: it only performs the runtime sandbox
        # check when it is given a cwd, and NO fixture passed one. The helper
        # existed, its docstring said "this runs", and it ran nowhere -- the
        # pattern this whole series is about, inside the fix for it.
        # (T3 review, #1647)
        if "sealed_env" in used:
            # The runtime check has to be CALLED, not merely available. Six of
            # six fixtures called it and the lint did not require it, so
            # deleting the call while keeping the import left both lints green.
            # calls_made() already existed here, aimed only at sealed_env.
            # (T3 review, #1647)
            if "inside_the_sandbox" not in used and "cwd" not in cwd_kwargs(tree):
                problems.append(
                    f"{path.name}: seals its config but never runs the sandbox "
                    "check -- neither sealed_env(cwd=...) nor "
                    "inside_the_sandbox(). Sealing stops a fixture READING the "
                    "real repository; only the runtime check stops it acting on "
                    "one git discovers by walking up.")
                continue
            if "cwd" not in cwd_kwargs(tree):
                problems.append(
                    f"{path.name}: calls sealed_env() but never with cwd=. "
                    "Without it the ceiling is not tied to the sandbox and the "
                    "runtime check does not run, so the file is sealed against "
                    "config and not against a repository git discovers by "
                    "walking up.")
            continue
        missing = [k for k in REQUIRED if k not in strings]
        if missing:
            problems.append(
                f"{path.name}: builds or configures a git repository and does not "
                f"seal the config surface ({', '.join(missing)} absent). Use "
                "`env=sealed_env()` from scripts/ci/fixture_env.py: dropping GIT_* "
                "stops a fixture reading the real repository, but not writing to "
                "the real config.")

    if checked == 0:
        print("[fixture-env] FATAL: no file was found that builds a repository. "
              "This lint has a population of zero and cannot report a clean "
              "result.", file=sys.stderr)
        return 2

    # THE SHELL FIXTURES, which this lint globbed past until 09-09-2026 (#495).
    #
    # A fixture written in bash reaches the real checkout through the same two
    # doors as one written in python -- a `cd` that failed and a git call with
    # no `-C` -- and it cannot be sealed the way a python one is: it drives
    # scripts that take their repository from the current directory, so the
    # environment has to stay real. What it can do is say afterwards whether the
    # checkout it started in still has the configuration it had at the start.
    #
    # That is what happened here twice in one afternoon: `core.bare = true` in
    # the main checkout, which broke `git rev-parse` in every one of its
    # worktrees, and then its `github` remote pointing at a URL that exists in
    # exactly one place in this repository -- a shell fixture's own throwaway
    # case. Neither could be reproduced by running the suspect on its own, and
    # neither fixture said anything at all.
    shell_problems: list[str] = []
    shell_checked = 0
    for path in sorted(MAP.glob("test_*.sh")):
        text = path.read_text(encoding="utf-8", errors="replace")
        code = "\n".join(line for line in text.splitlines()
                         if not line.lstrip().startswith("#"))
        if not re.search(r"(?:^|[;&|(`$]|\s)git\s+(?:-C\s+\S+\s+)?"
                         r"(?:init|config|remote|update-ref|checkout|commit)\b", code):
            continue
        shell_checked += 1
        # AND SEALED AGAINST THE ENVIRONMENT IT IS STARTED IN (#454). The
        # sentinel below says afterwards that the checkout was damaged; this
        # says the fixture cannot damage it. A git hook exports GIT_DIR, the
        # gate runs from the pre-push hook, and GIT_DIR outranks `-C`: every
        # `git -C /tmp/fixture …` then reads and writes the repository GIT_DIR
        # names, and `git -C /tmp/fixture init` under it re-initialises that
        # repository WITHOUT a work tree, which is written down as
        # `core.bare = true`. That one variable produced all four leaks of
        # 09-09-2026 -- bare twice, the master ref moved, the remote at
        # example.invalid, the identity at gate@invalid -- and the sentinel
        # reported none of them, because the fixtures that did it were not the
        # ones that carried the sentinel.
        # SOURCED, NOT MENTIONED. The same defect this repository has now found in
        # four guards: a plain substring search is satisfied by the prose that
        # explains the call -- and this file's own refusal message names the seal,
        # so deleting the line and keeping the message passed. Measured.
        if not re.search(r"^\s*(?:\.|source)\s+.*" + re.escape(SEAL), code, re.M):
            shell_problems.append(
                f"{path.name}: runs git and does not source {SEAL}. A hook exports "
                "GIT_DIR, GIT_DIR outranks `-C`, and this file is run from one: "
                "without the seal its git calls land in the repository the hook is "
                "about. One line, first thing after `set -uo pipefail`.")
        if SENTINEL not in code:
            shell_problems.append(
                f"{path.name}: builds or configures a git repository in shell and never "
                f"checks that the checkout it was started from is unchanged. Record "
                f"`bash {SENTINEL} --record` before the fixture runs and check it in the "
                f"EXIT trap; and give every git call a `-C <fixture>` so a failed cd "
                f"cannot fall through to the real one.")
    problems.extend(shell_problems)
    checked += shell_checked

    if problems:
        print(f"[fixture-env] FAIL: {len(problems)} of {checked} fixture file(s) "
              "can reach a real repository's config:", file=sys.stderr)
        for p in problems:
            print(f"    {p}", file=sys.stderr)
        print("\n  Two incidents in one night came from this, in two different\n"
              "  terminals: a fixture that ran init/commit on the real repository,\n"
              "  and a fixture identity written into a real worktree's config.",
              file=sys.stderr)
        return 1

    print(f"[fixture-env] OK: {checked} fixture file(s) build repositories, each "
          f"with the config surface sealed; {len(ALLOWED)} exempt with a reason.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
