#!/usr/bin/env python3
"""The hook writes the sign-off, and `format.signoff` does not.

Fixtures rather than assertions about the source, because the subject is what
git DOES. This exists because a setting everyone believed in did nothing:
`git config format.signoff true` was already set here and applies only to
`git format-patch`. `git commit` does not look at it.

That second case is the reason the hook exists, and without this test it lives
only in a commit message -- where nothing stops someone who later replaces the
hook with the config they assume works. (T1 review.)
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

HOOK = pathlib.Path(__file__).resolve().parents[2] / ".githooks" / "prepare-commit-msg"


def sealed(cwd=None) -> dict[str, str]:
    """The sealed environment for a fixture that builds a repository.

    Stripping GIT_* is not enough: the global and system config stay in play, and
    `git config user.email` inside a fixture still writes somewhere real.
    sealed_env closes all three surfaces and uses `cwd` as the ceiling, so git
    cannot find a real repository by walking upwards. That second half is the
    finding from #1647, and it applies to this test exactly as it applied to the
    fixtures I pointed it out on.
    """
    return sealed_env(cwd=cwd)


def git(*a: str, cwd: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                          text=True, env=sealed(cwd=cwd))


def trailers(repo: pathlib.Path) -> int:
    out = git("log", "-1", "--format=%B", cwd=repo).stdout
    return sum(1 for r in out.splitlines() if r.startswith("Signed-off-by:"))


def build(tmp: pathlib.Path, with_hook: bool, signoff_config: bool = False,
         name: str = "") -> pathlib.Path:
    repo = tmp / (name or ("with" if with_hook else "without"))
    repo.mkdir()
    git("init", "-q", ".", cwd=repo)
    git("config", "user.name", "proef", cwd=repo)
    git("config", "user.email", "proef@invalid", cwd=repo)
    if signoff_config:
        git("config", "format.signoff", "true", cwd=repo)
    if with_hook:
        d = repo / ".githooks"
        d.mkdir()
        (d / "prepare-commit-msg").write_bytes(HOOK.read_bytes())
        (d / "prepare-commit-msg").chmod(0o755)
        git("config", "core.hooksPath", ".githooks", cwd=repo)
    return repo


def main() -> int:
    fouten: list[str] = []

    # A missing hook is a finding, not a traceback. Without this the test died
    # with FileNotFoundError, which reads as "the test is broken" when the answer
    # is that its subject is gone.
    if not HOOK.is_file():
        print(f"test_prepare_commit_msg_signoff: {HOOK} does not exist, so there "
              "is nothing that writes the sign-off.", file=sys.stderr)
        return 1

    def expect(wat: str, ok: bool, detail: str = "") -> None:
        if not ok:
            fouten.append(f"{wat}{': ' + detail if detail else ''}")

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)

        # 1. Without the hook: no trailer. That is how you know the hook does
        #    it and not something else in the environment.
        without = build(tmp, with_hook=False)
        git("commit", "-q", "--allow-empty", "-m", "x", cwd=without)
        expect("without the hook there should be no sign-off", trailers(without) == 0,
            f"{trailers(without)} found")

        # 2. `format.signoff` alone: still no trailer. THIS is why the hook
        #    exists, and the only assertion that records it.
        config_only = build(tmp, with_hook=False, signoff_config=True, name="alleenconfig")
        (config_only / "f").write_text("x")
        git("add", "f", cwd=config_only)
        git("commit", "-q", "-m", "x", cwd=config_only)
        expect("format.signoff alleen voegt niets toe aan `git commit`",
            trailers(config_only) == 0,
            f"{trailers(config_only)} found -- if this ever becomes 1, the hook "
            "has become redundant and should go, not linger")

        # 3. With the hook: exactly one.
        with_hook_repo = build(tmp, with_hook=True)
        git("commit", "-q", "--allow-empty", "-m", "x", cwd=with_hook_repo)
        expect("with the hook there is one sign-off", trailers(with_hook_repo) == 1,
            f"{trailers(with_hook_repo)} found")

        # 4. Idempotent: an amend and an explicit -s double nothing.
        git("commit", "-q", "--amend", "--allow-empty", "--no-edit", cwd=with_hook_repo)
        expect("an --amend does not double the sign-off", trailers(with_hook_repo) == 1,
            f"{trailers(with_hook_repo)} found")
        git("commit", "-q", "--allow-empty", "-s", "-m", "with -s", cwd=with_hook_repo)
        expect("an explicit -s does not double the sign-off", trailers(with_hook_repo) == 1,
            f"{trailers(with_hook_repo)} found")

        # 5. Without an identity: no crash. `set -e` plus a bare
        #    `$(git config user.name)` kills the hook, and git then reports only
        #    "hook failed" -- its own explanation never reaches the user. (T1.)
        bare = tmp / "bare"
        bare.mkdir()
        git("init", "-q", ".", cwd=bare)
        d2 = bare / ".githooks"
        d2.mkdir()
        (d2 / "prepare-commit-msg").write_bytes(HOOK.read_bytes())
        (d2 / "prepare-commit-msg").chmod(0o755)
        git("config", "core.hooksPath", ".githooks", cwd=bare)
        r = subprocess.run(["git", "commit", "--allow-empty", "-m", "x"],
                           cwd=str(bare), capture_output=True, text=True,
                           env=dict(sealed(cwd=bare), GIT_AUTHOR_NAME="a", GIT_AUTHOR_EMAIL="a@b",
                                    GIT_COMMITTER_NAME="a", GIT_COMMITTER_EMAIL="a@b"))
        expect("without user.name the hook does not fail", r.returncode == 0,
            f"exit {r.returncode}: {(r.stderr or '').strip()[:120]}")

    if fouten:
        print("test_prepare_commit_msg_signoff: the hook does not do what it promises.\n",
              file=sys.stderr)
        for f in fouten:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("test_prepare_commit_msg_signoff: OK -- 6 case(s); the hook writes the "
          "sign-off, format.signoff does not.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
