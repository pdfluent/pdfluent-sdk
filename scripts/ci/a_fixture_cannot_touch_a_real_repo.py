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
import ast, pathlib, sys

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
