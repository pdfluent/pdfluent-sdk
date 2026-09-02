#!/usr/bin/env python3
"""The crash guard's classifier, driven rather than read.

The case this exists for: `timeout 30 ... || true` discarded the exit code and
judged each render from stderr alone. A process the timeout kills writes nothing
to stderr, so a PDF that hung the renderer scored as clean -- and a hang is the
outcome a crash guard is most obviously for.

Every case runs the real shell function, so a change that drops 124 back out of
the pattern turns this red instead of turning the gate quiet.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HIER = pathlib.Path(__file__).resolve().parent
SCRIPT = HIER / "classify_render_outcome.sh"
WORKFLOW = HIER.parent.parent / ".github/workflows/crash-guard.yml"

# (exit code, stderr text, expected verdict, why)
GEVALLEN = [
    (124, "", "hang", "the timeout fired and the process said nothing"),
    (124, "rendering page 1\n", "hang", "a hang is a hang even with chatter on stderr"),
    (137, "", "hang", "SIGKILL: timeout -k, or the OOM killer"),
    (0, "", "ok", "a clean render"),
    (0, "warning: unusual font matrix\n", "ok", "a warning is not a crash"),
    (1, "Error: PasswordProtected\n", "ok",
     "an ordinary non-zero exit is the binary behaving correctly"),
    (1, "Error: unsupported filter /JBIG2Decode\n", "ok", "same, a refusal is not a crash"),
    (101, "thread 'main' panicked at src/lib.rs:42\n", "crash", "a panic"),
    (134, "Aborted (core dumped)\n", "crash", "SIGABRT"),
    (139, "signal: 11 (SIGSEGV)\n", "crash", "SIGSEGV"),
    (1, "stack backtrace:\n   0: rust_begin_unwind\n", "crash",
     "a backtrace without the word panic is still a panic"),
    # A crash whose exit code happens to be 124 is judged a hang. That is the
    # intended precedence: both block the merge, and the code is the only
    # evidence that survives a kill.
    (124, "thread 'main' panicked at x.rs:1\n", "hang", "the code wins over the text"),
]

VLOER = 12


def roep(rc: int, err: str) -> str:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write(err)
        pad = f.name
    try:
        r = subprocess.run(
            ["sh", "-c", f'. "$1"; classify_render_outcome "$2" "$3"',
             "sh", str(SCRIPT), str(rc), pad],
            capture_output=True, text=True)
        if r.returncode != 0:
            return f"<the classifier itself exited {r.returncode}: {r.stderr.strip()}>"
        return r.stdout.strip()
    finally:
        pathlib.Path(pad).unlink(missing_ok=True)


def main() -> int:
    if not SCRIPT.is_file():
        print(f"SKIPPED (not a pass): {SCRIPT} is missing, so nothing was classified",
              file=sys.stderr)
        return 3
    if len(GEVALLEN) < VLOER:
        print(f"[classify] FATAL: {len(GEVALLEN)} cases, floor is {VLOER}. "
              "A shortened list is a quietly widened classifier.", file=sys.stderr)
        return 1

    fout = []
    for rc, err, verwacht, waarom in GEVALLEN:
        got = roep(rc, err)
        vlag = "ok  " if got == verwacht else "FAIL"
        print(f"  {vlag}  rc={rc:<4} -> {got:<5} ({waarom})")
        if got != verwacht:
            fout.append(f"exit {rc} with stderr {err!r}: expected {verwacht}, got {got} — {waarom}")

    # The classifier can be right and still not be used. The workflow has to
    # capture the exit code and hand it over; `|| true` on the render line is
    # exactly the shape that threw it away.
    if WORKFLOW.is_file():
        tekst = WORKFLOW.read_text(encoding="utf-8")
        if "classify_render_outcome" not in tekst:
            fout.append("crash-guard.yml does not call classify_render_outcome, "
                        "so this suite tests a classifier the gate does not use")
        if "2>/tmp/crash_err.txt || true" in tekst:
            fout.append("crash-guard.yml still discards the render exit code with "
                        "`|| true`, so a hang cannot reach the classifier")
        if "hangs=" not in tekst:
            fout.append("crash-guard.yml never reports a hang count, so a hang "
                        "would block the merge without saying why")
    else:
        print(f"SKIPPED (not a pass): {WORKFLOW} is missing, so the wiring was "
              "not checked", file=sys.stderr)

    print(f"[classify] {len(GEVALLEN)} case(s) + 3 wiring check(s)")
    if not fout:
        print("[classify] every outcome is judged as it should be")
        return 0
    print(f"[classify] {len(fout)} wrong:")
    for f in fout:
        print(f"  {f}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
