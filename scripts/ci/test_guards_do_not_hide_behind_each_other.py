#!/usr/bin/env python3
"""A guard step is classified by what it does, not by what it is called.

`is_prerequisite` decides whether a step in a guard job may abort the steps
below it. It used to concatenate `uses`, `run` and `name` and look for
substrings, so the exemption could be claimed by a step that merely MENTIONS a
setup action -- including a step whose whole purpose is to check that such an
action is pinned, and including a comment inside an unrelated command.

That matters because the exemption is the whole point of the file: an exempt
step is allowed to stop everything after it, and a step that never ran looks
exactly like a step that passed. A label granting that privilege means anyone
can grant it to themselves by naming a step well. (#321)

The cases below drive the real function with real step shapes.
"""
from __future__ import annotations

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
# The MODULE, not its names. Importing `PREREQUISITE_ACTIONS` directly makes
# this file die with an ImportError against any version that lacks it -- which
# is exactly the version whose behaviour it is supposed to expose. A test that
# cannot run against the code it accuses proves nothing about it.
import guards_do_not_hide_behind_each_other as bewaker  # noqa: E402

is_prerequisite = bewaker.is_prerequisite

gevallen: list[tuple[str, bool]] = []


def geval(wat: str, ok: bool, detail: str = "") -> None:
    gevallen.append((wat, ok))
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail}"))


def main() -> int:
    # The exemption still works for the steps it exists for. Without this the
    # cases below could all pass by making the function return False always,
    # which would turn every checkout into a reported problem.
    geval("a real checkout is a prerequisite",
          is_prerequisite({"uses": "actions/checkout@v4"}))
    geval("a real setup-python is a prerequisite",
          is_prerequisite({"uses": "actions/setup-python@v5", "name": "Python"}))
    geval("a real `pip install` line is a prerequisite",
          is_prerequisite({"run": "pip install -r requirements.txt"}))
    geval("a real `npm ci` line is a prerequisite",
          is_prerequisite({"run": "cd site\nnpm ci\n"}))

    # The defect. Each of these was exempt before #321.
    geval("a step NAMED after a setup action is not a prerequisite",
          not is_prerequisite({
              "name": "actions/checkout is pinned to a commit",
              "run": "python3 scripts/ci/pins_are_shas.py"}),
          "a step can exempt itself by choosing its own name")
    geval("a step whose name mentions pip install is not a prerequisite",
          not is_prerequisite({
              "name": "the runner image has pip install available",
              "run": "python3 scripts/ci/image_has_python.py"}),
          "name is being read as behaviour")
    geval("a COMMENT mentioning a setup command does not exempt the step",
          not is_prerequisite({
              "run": "# pip install is done in the image, not here\npython3 check.py"}),
          "a comment is not something the step runs")
    geval("a comment after a real command does not exempt it either",
          not is_prerequisite({"run": "python3 check.py   # npm ci lives in setup"}),
          "trailing comment read as a command")
    geval("an action whose id merely CONTAINS a prerequisite id is not one",
          not is_prerequisite({"uses": "third-party/actions-checkout-audit@v1"}),
          "substring match on the action id")

    # The comment stripper must not shorten real commands.
    strip = getattr(bewaker, "zonder_commentaar", None)
    geval("the classifier has a comment-aware line reader", strip is not None,
          "no zonder_commentaar(): run lines are being read raw")
    if strip is not None:
        geval("a `#` inside quotes survives",
              strip('echo "# heading"') == 'echo "# heading"',
              repr(strip('echo "# heading"')))
        geval("a `#` mid-word survives (a sha, a fragment)",
              strip("curl https://x/y#frag") == "curl https://x/y#frag",
              repr(strip("curl https://x/y#frag")))
        geval("a `#` starting a word is a comment",
              strip("run me   # not this").strip() == "run me",
              repr(strip("run me   # not this")))
        geval("an escaped quote does not end the string, so the comment goes",
              strip('echo "a\\"b"   # pip install').strip() == 'echo "a\\"b"',
              repr(strip('echo "a\\"b"   # pip install')))
        geval("a backslash inside single quotes is literal",
              strip("echo 'a\\' # gone").strip() == "echo 'a\\'",
              repr(strip("echo 'a\\' # gone")))

    # Wiring: the constant the function reads must still be the action list, so
    # a future edit cannot quietly reintroduce the concatenated blob.
    bron = (pathlib.Path(__file__).resolve().parent
            / "guards_do_not_hide_behind_each_other.py").read_text()
    geval("the classifier no longer reads `name`",
          "step.get('name'" not in bron.split("def is_prerequisite")[1].split("def ")[0]
          and 'step.get("name"' not in bron.split("def is_prerequisite")[1].split("def ")[0],
          "name is back in the classifier")
    acties = getattr(bewaker, "PREREQUISITE_ACTIONS", None)
    geval("the action list is separate from the command list", acties is not None,
          "no PREREQUISITE_ACTIONS: actions and commands share one substring blob")
    geval("actions/checkout is still in the action list",
          bool(acties) and "actions/checkout" in acties)

    mislukt = [w for w, ok in gevallen if not ok]
    print(f"\n  {len(gevallen)} assertion(s) ran, {len(mislukt)} failure(s)")
    return 1 if mislukt else 0


if __name__ == "__main__":
    sys.exit(main())
