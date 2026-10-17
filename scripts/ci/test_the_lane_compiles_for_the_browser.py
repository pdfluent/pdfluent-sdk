#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The wasm32 compile is in the landing lane, and it is the wave's compile (#483, #473).

WHY THIS FILE EXISTS

`scripts/ci/run_wasm_check.sh` catches a browser-only compile error before it
reaches master. It catches it because of three details, and every one of them can
be edited away in a diff that looks like a tidy-up:

  the lane runs it BEFORE MASTER TAKES   -- dropped from the gate, or moved back
  THE CHANGE                                onto the push lane where #473
                                            refuses a compiler outright
  it denies warnings                     -- without `-D warnings` a dead variant
                                            is a warning and the check is GREEN
                                            on the exact code that broke master
  it compiles the whole facade           -- narrowed to one crate it does not
                                            resolve getrandom's wasm backend and
                                            is red about somebody else's code
  the wave runs the same FILE            -- two spellings of one compile drift,
                                            and the lane's would be the one
                                            nobody re-derives

None of the three shows up as a failure anywhere: each leaves a gate that runs,
prints PASS, and has stopped answering. So they are asserted here, against both
files, rather than trusted to review.

WHAT IT DOES NOT DO

Compile anything. The compile is the gate itself (`wasmcheck` in the lane); this
is the guard over the gate, it reads two files, and it costs milliseconds.
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
LANE = REPO / "scripts" / "ci" / "local_ci_gate.sh"
CHECK = REPO / "scripts" / "ci" / "run_wasm_check.sh"
WAVE = REPO / ".github" / "workflows" / "ci-ephemeral.yml"

TARGET = "wasm32-unknown-unknown"

# The crate the check compiles, and therefore the crate the lane gates it on.
FACADE = "pdfluent"

FAILURES: list[str] = []


def judge(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        FAILURES.append(f"{name}{': ' + detail if detail else ''}")
        print(f"  FAIL {name}{': ' + detail if detail else ''}")


def code_lines(text: str) -> list[str]:
    """The command lines of a file: continuations folded, comments dropped.

    Both matter. The wave writes its command over two lines and this file's own
    reasoning quotes commands it decided against, so a matcher that read raw
    lines would compare half a command, or a footnote, with the real one.
    """
    folded = re.sub(r"\\\s*\n\s*", " ", text)
    return [ln.strip() for ln in folded.splitlines()
            if ln.strip() and not ln.strip().startswith("#")]


def cargo_check_words(text: str) -> list[list[str]]:
    """Every `cargo check` invocation in `text`, as a word list."""
    out = []
    for line in code_lines(text):
        if "cargo check" in line:
            out.append(line[line.index("cargo check"):].split())
    return out


def selection(words: list[str]) -> tuple[str, ...]:
    """What a `cargo check` line compiles: its target, packages and --tests."""
    keep: list[str] = []
    for i, word in enumerate(words):
        if word in {"--target", "-p", "--package"}:
            keep.append(f"{word} {words[i + 1]}" if i + 1 < len(words) else word)
        elif word in {"--tests", "--workspace", "--all-targets", "--lib"}:
            keep.append(word)
    return tuple(sorted(keep))


def main() -> int:
    for path in (LANE, CHECK, WAVE):
        if not path.is_file():
            print(f"the-lane-compiles-for-the-browser: {path} is missing", file=sys.stderr)
            return 2

    lane = LANE.read_text(encoding="utf-8")
    check = CHECK.read_text(encoding="utf-8")
    wave = WAVE.read_text(encoding="utf-8")

    # 1. In the lane at all, and as `zwaar` -- the landing lane and the full one.
    #
    #    It was `run` from #483 until #473, on the argument that the author of a
    #    broken cfg is at the keyboard at push time. Since #474 the gate does not
    #    run at that keyboard: it runs on an instance bought for the push, the
    #    finding comes back over ssh either way, and 27 s of every push on that
    #    instance bought an answer the landing lane gives again before anything
    #    lands. What #483 was written against is untouched by the move: master
    #    takes only fast-forwards, so no change reaches it without the landing
    #    lane, and the two hours of red came from finding this on the WAVE.
    #
    #    What is given up, precisely: a browser-only break is now found at the
    #    landing rather than at the push before it. `run` is refused here, and
    #    not only for the cost -- a_push_gate_does_not_compile_the_workspace.py
    #    fails the push lane outright when a compiler appears on it.
    lane_lines = [ln.strip() for ln in lane.splitlines()
                  if "scripts/ci/run_wasm_check.sh" in ln and not ln.strip().startswith("#")]
    judge("the lane calls run_wasm_check.sh", bool(lane_lines),
          "no line in local_ci_gate.sh runs it")
    #
    #    `crate_gate` counts as well as `zwaar` (#523). Both defer on a push --
    #    a_push_gate_does_not_compile_the_workspace.py reads the bodies and
    #    refuses either of them if it stops saying DEFERRED -- and `crate_gate`
    #    adds the one thing this gate could not say before: the command it runs
    #    is `cargo check -p pdfluent --target wasm32`, so a landing that moves
    #    no crate of that graph was compiling for the browser to answer a
    #    question nothing had asked. What is refused here is `run`, which would
    #    put a compiler on the push lane.
    #
    #    The crate is part of the claim, not decoration: `crate_gate wasmcheck
    #    visual-regression` would leave this test green while a landing that
    #    changes `pdfluent` skips the browser compile altogether. So the whole
    #    prefix is asserted, crate and all. (codex, #1879)
    judge("it is in the landing lane, before master takes the change",
          all(ln.startswith("zwaar ") or ln.startswith(f"crate_gate wasmcheck {FACADE} ")
              for ln in lane_lines),
          f"lines: {lane_lines}")

    # 2. Warnings are denied. This is the whole of what makes a dead variant
    #    red: cargo returns 0 on a warning.
    denies = [ln for ln in code_lines(check) if re.search(r"RUSTFLAGS=\S*.?-D warnings", ln)]
    judge("the check denies warnings", bool(denies),
          "no command line in run_wasm_check.sh sets RUSTFLAGS=-D warnings, so a "
          "dead variant is a warning and the gate is green on it")

    # 3. It compiles the WHOLE facade for wasm32, not one crate of it. The
    #    narrow `-p pdf-manip` does not resolve getrandom's wasm backend at all,
    #    so a check narrowed back to it is red about somebody else's crate --
    #    and a gate that is red for the wrong reason is switched off in a week.
    ours = [selection(w) for w in cargo_check_words(check) if f"--target {TARGET}" in " ".join(w)]
    judge("the check compiles for wasm32", bool(ours),
          "no wasm32 cargo check in run_wasm_check.sh")
    judge("it compiles the facade and its tests",
          (f"--target {TARGET}", "--tests", "-p pdfluent") in
          [tuple(sorted(sel)) for sel in ours] or
          any({"-p pdfluent", "--tests", f"--target {TARGET}"} <= set(sel) for sel in ours),
          f"what it compiles is {ours}")

    # 4. The wave runs the SAME FILE. Not a second command that says the same
    #    thing today: that is what these two were until #483, and the local half
    #    was the half that did not exist.
    judge("the wave runs the same script",
          "scripts/ci/run_wasm_check.sh" in wave,
          "ci-ephemeral.yml no longer runs run_wasm_check.sh, so the wave and the "
          "lane can drift apart again")

    if FAILURES:
        print("the-lane-compiles-for-the-browser: FAIL")
        for line in FAILURES:
            print(f"  - {line}")
        return 1
    print("the-lane-compiles-for-the-browser: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
