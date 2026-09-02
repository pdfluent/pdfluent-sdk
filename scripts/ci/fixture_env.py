#!/usr/bin/env python3
"""A sealed environment for tests that build throwaway git repositories.

Two incidents on one night, in two terminals, from the same cause:

  A fixture helper inherited GIT_DIR from a pre-push hook and ran `init` and
  `commit` against the REAL repository. Recovered from the reflog.

  A fixture's `git config user.email t@t` landed in a real worktree's local
  config, and every rebase there afterwards stamped commits with a test
  identity. Caught by the commit-identity gate, one push later.

Neither was a bug in the code under test. Both were the fixture reaching out of
its sandbox, and `env=` alone does not stop the second: dropping GIT_* still
leaves the user's global and system config in play, and a fixture that writes
config can still write it somewhere real.

Sealing means three things, and all three matter:

  GIT_* dropped          -- so `-C` and cwd decide which repository git reads
  GIT_CONFIG_NOSYSTEM=1  -- so /etc/gitconfig cannot change the answer
  GIT_CONFIG_GLOBAL      -- pointed at an empty temp file, so the user's ~/.gitconfig
                            neither leaks in nor gets written to

Use it as `env=sealed_env()` on every git call in a fixture.
"""
from __future__ import annotations
import atexit, os, pathlib, subprocess, tempfile

_EMPTY: str | None = None


def _empty_config() -> str:
    """One empty file per process, removed on exit."""
    global _EMPTY
    if _EMPTY is None or not os.path.exists(_EMPTY):
        fd, path = tempfile.mkstemp(prefix="pdfluent-empty-gitconfig-")
        os.close(fd)
        _EMPTY = path
        atexit.register(lambda p=path: os.path.exists(p) and os.unlink(p))
    return _EMPTY


def sealed_env(identity: bool = False, cwd: str | os.PathLike | None = None,
               **extra: str) -> dict[str, str]:
    """The caller's environment with git's own inputs removed and pinned."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env["GIT_CONFIG_GLOBAL"] = _empty_config()
    # The child guard in test_a_gate_that_never_went_green.py picks its live
    # urllib route when a token is present instead of the fake `gh` the fixture
    # installs. The helper it replaced stripped these on purpose.
    for tok in ("GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN"):
        env.pop(tok, None)
    if identity:
        env.update({"GIT_AUTHOR_NAME": "fixture", "GIT_AUTHOR_EMAIL": "fixture@invalid",
                    "GIT_COMMITTER_NAME": "fixture", "GIT_COMMITTER_EMAIL": "fixture@invalid"})
    # NO identity defaults here, and that was a real mistake before it was a
    # comment. Setting GIT_AUTHOR_EMAIL and friends puts an ENVIRONMENT value in
    # play, and environment beats config -- so it silently overrode the identity
    # fixtures configure on purpose. test_commits_use_the_noreply_alias.py builds
    # a repo whose commits are supposed to carry a noreply alias, and a default
    # of "fixture@invalid" made its own clean case fail.
    #
    # A fixture that needs an identity sets one in its sandbox, which is safe
    # now: with the global config sealed and cwd inside a temp repository, a
    # `git config` write lands in that repository and nowhere else. Ask for
    # env-level identity explicitly with identity=True when a test is ABOUT the
    # environment rather than the config.
    # `extra` FIRST, seals after. The other order let
    # sealed_env(GIT_CONFIG_GLOBAL="~/.gitconfig") return an unsealed
    # environment -- incident 2 straight through the helper written to prevent
    # it, and both lints approve the call because it says `env=sealed_env(...)`.
    # (T3 review, #1647)
    # The SAME prefix filter that is applied to the inherited environment one
    # line above. Naming two keys by hand let nine others through, including
    # GIT_DIR -- incident 1, the first sentence of this file -- and
    # GIT_CONFIG_COUNT/GIT_CONFIG_KEY_0/GIT_CONFIG_VALUE_0, which set config
    # through the environment and so walk around the sealed global file:
    # incident 2 through a different door. Stripping GIT_* from what we inherit
    # and then accepting GIT_* from the caller is not a filter, it is a
    # doorman who checks the front and holds the back open.
    # (T3 review, #1647)
    smuggled = sorted(k for k in extra if k.startswith("GIT_"))
    if smuggled:
        raise ValueError(
            "sealed_env() will not take " + ", ".join(smuggled) + " from a "
            "caller: git's own variables are exactly what this helper removes. "
            "A fixture that needs one is asking for the environment this exists "
            "to deny. Use identity=True for an author, or cwd= for a sandbox.")
    env.update(extra)
    env["GIT_CONFIG_NOSYSTEM"] = "1"
    env["GIT_CONFIG_GLOBAL"] = _empty_config()
    # Always set, not only when a cwd is given -- no caller passed one, so the
    # ceiling was never set at all. Without a cwd the sandbox root is the
    # system temp directory, which is where fixtures build repositories.
    root = pathlib.Path(cwd).resolve().parent if cwd is not None \
        else pathlib.Path(tempfile.gettempdir()).resolve()
    env["GIT_CEILING_DIRECTORIES"] = str(root)
    if cwd is not None:
        # Passing a cwd IS the runtime check. inside_the_sandbox() was written
        # to run and then called from nowhere -- its docstring said "this runs"
        # while zero fixtures used it. Hanging it here means one place enforces
        # and one place is enforced, instead of six call sites to remember.
        # (T3 review, #1647)
        inside_the_sandbox(cwd, _env=env)
    return env


def inside_the_sandbox(cwd: str | os.PathLike, _env: dict | None = None) -> None:
    """Refuse before git writes anything outside `cwd`.

    A lint reads code; this runs. If the repository git would act on is not
    under `cwd`, the fixture is about to configure or commit somewhere real, and
    the honest moment to stop is before the first write, not after the gate
    notices the identity in a later commit. (codex, #1647)
    """
    # _env breaks the recursion: sealed_env(cwd=…) calls this, so building a
    # fresh environment here would call sealed_env again, for ever.
    #
    # The ceiling is REMOVED for the probe, deliberately. With it in place git
    # stops walking at the sandbox root, finds nothing, and the check reports
    # "no repository here yet" for a cwd sitting inside the real checkout -- the
    # protection blinding the detection. The question being asked is what git
    # WOULD reach without the ceiling, because that is what an unsealed call in
    # the same directory would reach.
    env = dict(_env) if _env is not None else sealed_env()
    env.pop("GIT_CEILING_DIRECTORIES", None)
    here = pathlib.Path(cwd).resolve()
    # A fixture works in a temporary directory. Anywhere else, whatever git
    # finds is somebody's real repository -- including the case where the cwd IS
    # its root, which an earlier version of this check accepted and which is the
    # incident exactly: a fixture at the checkout root writes to the checkout's
    # own config.
    tmp = pathlib.Path(tempfile.gettempdir()).resolve()
    if not (here == tmp or tmp in here.parents):
        raise RuntimeError(
            f"a fixture would run in {here}, which is not under {tmp}. Build "
            "throwaway repositories in a temporary directory: outside one, any "
            "repository git finds is a real one.")
    r = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(cwd),
                       capture_output=True, text=True, env=env)
    if r.returncode != 0:
        return  # no repository here yet: `git init` is about to make one
    top = pathlib.Path(r.stdout.strip()).resolve()
    root = pathlib.Path(cwd).resolve()
    # The repository must be AT or UNDER the sandbox. The first rule also
    # accepted one discovered ABOVE it ("top in root.parents"), which is the
    # incident itself: a fixture run from inside the real checkout found the real
    # repository and was waved through. Discovery upward is the thing being
    # stopped, so it cannot be the thing that satisfies the check.
    if not (root == top or root in top.parents):
        raise RuntimeError(
            f"a fixture in {root} would act on the repository at {top}, which is "
            "outside its sandbox. Pass cwd= pointing inside a temporary "
            "directory; sealing the config surface does not protect a repository "
            "git discovers by walking up.")


def gate_aanroepen(script: str, wortel=None) -> list[str]:
    """Lines of scripts/ci/local_ci_gate.sh that run EXACTLY this script.

    Written once and shared, because the obvious form of this check is wrong in
    a way that reads as right: `"x.py" in gate_text` is satisfied by
    `test_x.py`, so an assertion that the gate still runs a guard stays green
    when only its TEST is wired -- which is exactly the state the assertion
    exists to detect. Measured on #1660: deleting the guard's own invocation
    left the suite at 10 passed, 0 failed.

    Compared per whitespace-separated argument on its basename, so `python3
    scripts/ci/x.py --flag` matches `x.py` and `test_x.py` never does.
    """
    import pathlib as _p
    wortel = _p.Path(wortel) if wortel else _p.Path(__file__).resolve().parents[2]
    poort = wortel / "scripts" / "ci" / "local_ci_gate.sh"
    uit = []
    for regel in poort.read_text(errors="replace").splitlines():
        if any(stuk.rsplit("/", 1)[-1] == script for stuk in regel.split()):
            uit.append(regel)
    return uit
