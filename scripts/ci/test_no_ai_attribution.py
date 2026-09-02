#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Prove that both layers of the AI-attribution rule actually bite.

`no_ai_attribution.py` and the `commit-msg` hook shipped without a test. That is
the exact failure this project has hit three times: the check exists and nothing
runs it, or it runs and cannot fail. Here the two layers are asserted separately,
because they fail in different ways and one masks the other:

    layer 1  the hook rewrites the message before the commit exists.
             If it is broken, layer 2 still passes -- because the hook is what
             keeps the range clean, so a dead hook looks like a clean repo.

    layer 2  the guard refuses a range that carries the trailer.
             If it is broken, layer 1 still looks fine in daily use -- you only
             find out on the day a commit arrives from a machine without hooks.

Every case is built in a throwaway repository under a temporary directory, with
the GIT_* variables stripped (see test_no_test_can_touch_the_real_repo.py, and
#240 for what happens without that).

# FLOOR: cases >= 8 -- if this file ever asserts fewer, it has stopped covering
# both layers, their negative cases, and the carrying of the hook itself.
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GUARD = ROOT / "scripts" / "ci" / "no_ai_attribution.py"
HOOK = ROOT / "scripts" / "git-hooks" / "commit-msg"

TRAILER = "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"


def _git_env(cwd=None) -> dict[str, str]:
    """Environment for git calls, from `fixture_env.sealed_env(identity=True)`.

    Stripping `GIT_*` was this file's own idea and #1647's lint is right that it
    is half the job: it stops a fixture READING the real repository and does
    nothing about it WRITING to the real config. `identity=True` because these
    fixtures are about what a commit's message contains, so the author has to
    come from the environment rather than from a config this seals away.
    """
    import importlib.util as _ilu
    _spec = _ilu.spec_from_file_location(
        "fixture_env", Path(__file__).resolve().parent / "fixture_env.py")
    _fx = _ilu.module_from_spec(_spec)
    _spec.loader.exec_module(_fx)
    # `cwd=` is what makes sealed_env run the sandbox check, so a fixture
    # pointed anywhere but a temporary directory fails loudly.
    e = _fx.sealed_env(identity=True, cwd=cwd) if cwd is not None \
        else _fx.sealed_env(identity=True)
    return e


def _git(work: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess:
    r = subprocess.run(["git", "-C", str(work), *args],
                       env=_git_env(), capture_output=True, text=True)
    if check and r.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed in {work}:\n{r.stderr}")
    return r


def _new_repo(work: Path, *, with_hook: bool) -> None:
    """A scratch repository with `master` and one clean commit on it.

    The hook is copied in rather than symlinked: the point is to exercise the
    hook's own logic, not the repository's symlink layout, and an absolute
    symlink is precisely the bug that made layer 1 dead outside one machine.
    """
    _git(work, "init", "--initial-branch=master", "-q")
    (work / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
    (work / "scripts" / "git-hooks").mkdir(parents=True, exist_ok=True)
    (work / "scripts" / "ci" / "no_ai_attribution.py").write_bytes(GUARD.read_bytes())
    if with_hook:
        hooks = work / ".githooks"
        hooks.mkdir(exist_ok=True)
        dest = hooks / "commit-msg"
        dest.write_bytes(HOOK.read_bytes())
        dest.chmod(0o755)
        _git(work, "config", "core.hooksPath", ".githooks")
    (work / "a.txt").write_text("a\n")
    _git(work, "add", "-A")
    _git(work, "commit", "-q", "-m", "chore: base")


def _commit(work: Path, message: str, *, no_verify: bool = False) -> None:
    (work / "a.txt").write_text(message + "\n")
    _git(work, "add", "a.txt")
    args = ["commit", "-q", "-m", message]
    if no_verify:
        args.insert(1, "--no-verify")
    _git(work, *args)


def _last_message(work: Path) -> str:
    return _git(work, "log", "-1", "--format=%B").stdout


def _run_guard(work: Path, rng: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(work / "scripts" / "ci" / "no_ai_attribution.py"),
         "--range", rng],
        cwd=str(work), env=_git_env(work), capture_output=True, text=True)


CASES: list[tuple[str, bool]] = []


def case(name: str, ok: bool, detail: str = "") -> None:
    CASES.append((name, ok))
    print(f"  {'PASS' if ok else 'FAIL'}  {name}{('  -- ' + detail) if detail and not ok else ''}")


def main() -> int:
    print("[no-ai-attribution] proving both layers bite")
    with tempfile.TemporaryDirectory(prefix="no-ai-attr-") as tmp:
        # --- layer 1: the hook strips, silently, before the commit exists ------
        w = Path(tmp) / "hooked"
        w.mkdir()
        _new_repo(w, with_hook=True)
        _commit(w, "feat: something\n\n" + TRAILER)
        msg = _last_message(w)
        case("layer 1 strips the trailer at commit time",
             "Co-Authored-By" not in msg, repr(msg))
        case("layer 1 keeps the subject intact", msg.startswith("feat: something"), repr(msg))

        # The hook must not eat a message that carries no attribution.
        _commit(w, "fix: untouched")
        case("layer 1 leaves a clean message alone",
             _last_message(w).startswith("fix: untouched"), repr(_last_message(w)))

        # --- layer 2: the guard refuses what got past the hook -----------------
        u = Path(tmp) / "unhooked"
        u.mkdir()
        _new_repo(u, with_hook=False)
        base = _git(u, "rev-parse", "HEAD").stdout.strip()

        _commit(u, "fix: clean one")
        clean = _run_guard(u, f"{base}..HEAD")
        case("layer 2 passes a clean range", clean.returncode == 0,
             clean.stdout + clean.stderr)

        _commit(u, "feat: sneaked in\n\n" + TRAILER, no_verify=True)
        dirty = _run_guard(u, f"{base}..HEAD")
        case("layer 2 fails a range carrying the trailer", dirty.returncode == 1,
             f"exit={dirty.returncode} {dirty.stdout} {dirty.stderr}")
        case("layer 2 names the offending commit",
             "sneaked in" in (dirty.stderr + dirty.stdout),
             dirty.stderr + dirty.stdout)

        # A form the hook was written for that is not `Co-Authored-By`.
        g = Path(tmp) / "generated"
        g.mkdir()
        _new_repo(g, with_hook=False)
        gbase = _git(g, "rev-parse", "HEAD").stdout.strip()
        _commit(g, "chore: tidy\n\n\U0001f916 Generated with [Claude Code](https://claude.com/claude-code)",
                no_verify=True)
        gen = _run_guard(g, f"{gbase}..HEAD")
        case("layer 2 catches the non-trailer 'Generated with' form",
             gen.returncode == 1, f"exit={gen.returncode} {gen.stderr}")

        # --- layer 1 has to survive being carried by the repository ----------
        #
        # The hook only travels with a clone because `.githooks/commit-msg` is a
        # RELATIVE symlink. It was absolute until 30-08-2026, pointing into one
        # worktree on one machine, so every other checkout got a dangling link
        # and no layer 1 at all. And `install.sh` wrote that absolute link back
        # into the tracked tree on 31-08 -- the installer reintroducing the
        # defect it exists to prevent. Both are asserted here because neither
        # shows up as a failure anywhere else: a dead hook looks like a clean
        # repository.
        # --- the forms added on 02-09-2026 ------------------------------------
        #
        # Three came from a coverage comparison between this guard and a second
        # one written the same night: `Co-authored-by: Cursor Agent` (the name
        # without a domain), and `Created by` / `Generated by` without a colon.
        # The fourth, Devin, was caught by NEITHER, and that is the interesting
        # one: both guards held a list of the assistants their author happened
        # to know. A list of names ages with the market.
        #
        # Settled 02-09-2026, and more cleanly than the allowlist first
        # sketched here: EVERY `Co-authored-by:` is stripped, whatever address
        # it names. The standing rule is that the trailer is never added, so
        # there is nothing to allow -- and a rule that inspects no address
        # cannot be outrun by a product that ships next month. The prose
        # patterns stay as a second, openly incomplete layer.
        import importlib.util as _ilu
        _spec = _ilu.spec_from_file_location(
            "guard", ROOT / "scripts" / "ci" / "no_ai_attribution.py")
        _guard = _ilu.module_from_spec(_spec)
        _spec.loader.exec_module(_guard)
        for regel, moet_vangen in [
            ("Co-authored-by: Cursor Agent <agent@cursor.sh>", True),
            ("Created by Claude", True),
            ("Generated by Anthropic Claude", True),
            ("Co-authored-by: Devin <devin@cognition.ai>", True),
            # The other half of the pair: these must NOT be caught, or the
            # widened patterns have bought coverage with false positives.
            ("Signed-off-by: A Person <person@example.invalid>", False),
            ("perf: the claude-opus-5 model id moved to a constant", False),
            # Caught, and deliberately so. The standing rule is that the
            # trailer is never added, so there is no legitimate Co-authored-by
            # in this repository -- not even one naming a person. That is what
            # makes the rule closed: no address is inspected, so no product
            # released tomorrow can walk around it.
            ("Co-authored-by: A Person <person@example.invalid>", True),
            ("docs: generated with the same seed as the fixture", False),
        ]:
            gevangen = bool(_guard.is_fout(regel))
            case(f"{'catches' if moet_vangen else 'leaves alone'}: {regel[:46]}",
                 gevangen == moet_vangen, repr(regel))

        link = ROOT / ".githooks" / "commit-msg"
        target = os.readlink(link) if link.is_symlink() else ""
        case("the tracked commit-msg hook is a relative symlink",
             link.is_symlink() and not os.path.isabs(target), repr(target))
        case("the tracked commit-msg hook resolves", link.exists(), repr(target))

        before = target
        # No `cwd=` here, deliberately: this case runs the installer against the
        # REAL repository, because what it asserts is that the installer leaves
        # the tracked symlink alone. Passing cwd would run the sandbox check,
        # which refuses any directory outside a temporary one -- and it would be
        # right to: the check exists to stop a fixture touching a real
        # repository. This is not a fixture, it is the one case whose subject is
        # the real tree, and it only reads and re-links what is already there.
        inst = subprocess.run(["bash", str(ROOT / "scripts" / "git-hooks" / "install.sh")],
                              cwd=str(ROOT), env=_git_env(), capture_output=True, text=True)
        after = os.readlink(link) if link.is_symlink() else ""
        case("install.sh leaves the tracked symlink alone",
             after == before and inst.returncode == 0,
             f"exit={inst.returncode} {before!r} -> {after!r}")

    failed = [n for n, ok in CASES if not ok]
    if len(CASES) < 8:
        print(f"FLOOR: only {len(CASES)} cases asserted, expected at least 8", file=sys.stderr)
        return 1
    if failed:
        print(f"\n{len(failed)} of {len(CASES)} case(s) failed:", file=sys.stderr)
        for n in failed:
            print(f"  {n}", file=sys.stderr)
        return 1
    print(f"OK: {len(CASES)} cases, both layers bite.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
