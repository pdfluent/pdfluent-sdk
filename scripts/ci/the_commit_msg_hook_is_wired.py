#!/usr/bin/env python3
"""The commit-msg hook exists, calls both guards, and is actually installed.

`docs/HERKOMST.md` claimed since 26-08-2026 that AI attribution "cannot get in
any more: scripts/ci/no_ai_attribution.py plus a commit-msg hook". Measured on
02-09-2026: `core.hooksPath` was `.githooks`, that directory held `pre-commit`
and `pre-push` and no `commit-msg` at all, and the guard existed on an unmerged
branch. Four months of commits under a rule that enforced nothing, and a
document saying it did.

So this checks the wiring rather than the intention, in three separate layers,
because they fail independently:

1. THE FILE. `.githooks/commit-msg` exists and is executable.
2. THE CALLS. It names both guards. A hook that runs one of them is a hook that
   silently stopped covering the other.
3. THE INSTALLATION. `core.hooksPath` actually points at the directory holding
   it. This is the layer nobody checks, and it is per clone: a guard that only
   looks for the file proves the file exists, not that git will ever run it.
   A fresh clone has `core.hooksPath` unset and every hook in this repository
   is inert until someone runs the installer.

Layer 3 is reported separately from 1 and 2 on purpose. A tree can be correct
while a clone is not configured, and those are different problems with different
fixes -- conflating them would either fail CI for a local misconfiguration or
pass a clone where the hook never runs.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
HAAK = REPO / ".githooks" / "commit-msg"
VEREIST = ("no_ai_attribution.py", "geen_interne_zaken.py")


def schone_omgeving() -> dict[str, str]:
    """From `fixture_env.sealed_env()`: `GIT_*` dropped and the config sealed.

    This guard only READS `core.hooksPath`, which is local config and unaffected
    by the seal. It goes through the shared helper anyway, because a second way
    of building a git environment is how the two drift apart -- and because the
    lint that requires it is right about the general case even where this
    particular call is harmless.
    """
    import importlib.util as _ilu
    _spec = _ilu.spec_from_file_location(
        "fixture_env", Path(__file__).resolve().parent / "fixture_env.py")
    _fx = _ilu.module_from_spec(_spec)
    _spec.loader.exec_module(_fx)
    return _fx.sealed_env()


def main() -> int:
    problemen: list[str] = []

    # 1. the file
    if HAAK.is_symlink() and not HAAK.resolve().exists():
        problemen.append(
            f"{HAAK.relative_to(REPO)} is a symlink to "
            f"{os.readlink(HAAK)}, which does not exist. Checked before "
            "existence, because Path.exists() follows the link and would "
            "report the far more confusing 'does not exist' about a file that "
            "is plainly there.")
    elif not HAAK.exists():
        problemen.append(
            f"{HAAK.relative_to(REPO)} does not exist. Every message guard in "
            "this repository runs from it; without it they run nowhere, and a "
            "rule nothing enforces is a rule that is not there.")
    elif not os.access(HAAK, os.X_OK):
        problemen.append(
            f"{HAAK.relative_to(REPO)} is not executable, so git will not run "
            "it. This fails silently: git skips a hook it cannot execute and "
            "says nothing.")
    else:
        # 2. the calls
        # Comment lines are stripped before looking. A guard's NAME appears in
        # this hook's own explanation of why it calls it, so a plain substring
        # search over the file is satisfied by the prose that describes the
        # call -- delete the call, keep the comment, and the check stays green.
        # Measured: that mutation passed until this line existed. It is the
        # same defect this repository has found in three other guards tonight,
        # and writing one myself is the reason it is worth saying twice.
        tekst = HAAK.read_text(errors="replace")
        regels = [r for r in tekst.splitlines()
                  if not r.lstrip().startswith("#") and "python3" in r]
        for guard in VEREIST:
            if not any(guard in r for r in regels):
                problemen.append(
                    f"the commit-msg hook does not call {guard}. Both guards "
                    "read the message and neither can be reached any other "
                    "way before the commit exists.")

    if problemen:
        print("[commit-msg-wiring] the message guards do not run:\n", file=sys.stderr)
        for p in problemen:
            print(f"  - {p}", file=sys.stderr)
        return 1

    # 3. the installation, reported separately
    uit = subprocess.run(["git", "config", "--get", "core.hooksPath"],
                         cwd=REPO, capture_output=True, text=True,
                         env=schone_omgeving(), check=False)
    pad = uit.stdout.strip()
    if pad != ".githooks":
        print(f"[commit-msg-wiring] the tree is correct and THIS CLONE is not: "
              f"core.hooksPath is {pad or 'unset'}, so git does not run "
              f".githooks/commit-msg here.\n\n"
              f"  Fix it for this clone:\n\n"
              f"    git config core.hooksPath .githooks\n\n"
              f"  It is per clone and per worktree checkout, which is why it is "
              f"checked rather than assumed. Nothing in the repository can set "
              f"it for you.", file=sys.stderr)
        return 1

    print("[commit-msg-wiring] OK: .githooks/commit-msg exists, calls "
          f"{' and '.join(VEREIST)}, and core.hooksPath points at it")
    return 0


if __name__ == "__main__":
    sys.exit(main())
