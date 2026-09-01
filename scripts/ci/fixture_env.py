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
import atexit, os, tempfile

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


def sealed_env(identity: bool = False, **extra: str) -> dict[str, str]:
    """The caller's environment with git's own inputs removed and pinned."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env["GIT_CONFIG_NOSYSTEM"] = "1"
    env["GIT_CONFIG_GLOBAL"] = _empty_config()
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
    env.update(extra)
    return env
