#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Nothing a push runs may reach a compiler. (#473)

WHAT THIS IS ABOUT

A change is compiled twice: once by the gate the push runs, and once by the gate
the landing runs. The second one is the one that decides -- master takes only
fast-forwards, so what passes the landing lane is what lands. The first one
therefore buys nothing that the second does not buy again half an hour later,
and it is the expensive half of every push: on 09-09-2026 the fast lane was
395 s on a quiet cpx42, of which `cargo check` for wasm32 and a `cargo run` that
regenerates a Java fixture were 74 s between them -- more than the hundred and
thirty guards that read files.

That is not a fact about those two gates. It is what a compiler costs, and the
lane will drift back the moment somebody adds a gate that "only checks one
crate": a cargo invocation on a cold instance is minutes, and the person adding
it measures it on a machine where the target directory is warm.

So the rule is mechanical rather than remembered: a gate in the fast lane may
not reach `cargo build`, `test`, `check`, `run`, `clippy`, `bench`, `doc` or
`rustc`, directly or through the scripts it calls. The landing lane compiles;
the push lane reads.

WHAT COUNTS AS THE FAST LANE

The gate script has four helpers and only one of them runs on a push:

    run          runs in every lane, including the fast one   <- this file
    zwaar        deferred on a push, runs at landing
    crate_gate   deferred on a push, runs at landing for its crate
    scoped       deferred on a push, runs at landing for the crates it is given

That mapping is checked here rather than assumed, because it is exactly the kind
of thing a later change makes untrue quietly: a `zwaar` that stopped deferring
would put every heavy gate back on every push, and this guard would go on
reporting a clean fast lane because it was reading a list that no longer means
what its name says.

WHAT COUNTS AS REACHING A COMPILER

Text alone would be wrong in both directions. Half the guards in this directory
carry `cargo test` inside a fixture string or a comment -- nine of them at the
time of writing -- and a guard that read those would be red about documentation.
So:

  Python is read as a syntax tree, and only an actual subprocess invocation
  counts: `subprocess.run(["cargo", "build", ...])` is a compile,
  `f"    cargo run -p pdf-manip ..."` in an error message is a sentence.

  Shell is read line by line with comments dropped, and only a command in
  command position counts.

  A script that runs another script is followed, up to three hops, so that
  hiding the compiler one file deeper does not hide it from this. Only paths in
  COMMAND position are followed: a guard that READS local_ci_gate.sh (three of
  them do) is not running it, and following every path a file merely mentions
  turned this check into thirty findings about files nobody executes.

Exit codes:
  0  no gate on the push lane can reach a compiler
  1  one can, and the finding names the gate, the file and the line
  2  the gate script could not be read, or the scan found so few gates that it
     is not reading what it thinks it is -- a scan that finds nothing approves
     everything
"""
from __future__ import annotations

import ast
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts" / "ci" / "local_ci_gate.sh"

# The cargo subcommands that put rustc to work. `metadata`, `fmt`, `--version`,
# `locate-project` and `tree` are not here on purpose: they read the manifest or
# the source text and are milliseconds, which is why `fmt` and `metadata` are
# the two cargo lines the fast lane keeps.
COMPILING = {
    "build", "b",
    "test", "t",
    "check", "c",
    "run", "r",
    "bench",
    "clippy",
    "doc", "rustdoc",
    "rustc",
    "install",
    "nextest",
    "miri",
}

# The helpers of local_ci_gate.sh that DEFER on a push, with the word each one's
# body must still print. Read from the file below; the pairing is the claim.
DEFERRING = {
    "zwaar": "DEFERRED",
    "crate_gate": "DEFERRED",
    "scoped": "DEFERRED",
}

# A scan that finds nothing approves everything. The fast lane held about a
# hundred and forty `run` gates when this was written; a floor well under that
# catches a broken regex without failing on an afternoon's honest pruning.
MIN_GATES = 60

# Interpreters: a path after one of these is a script being RUN, not a file
# being read. `sh -c` is deliberately absent -- what follows it is a program,
# not a path, and the scripts it names are picked up by the same rules anyway.
INTERPRETERS = {"bash", "sh", "python", "python3", "node", "zsh"}

# Paths inside the repository, as they are written in a command line.
PATH_IN_TEXT = re.compile(r"(?:scripts|benchmarks|crates|tools)/[A-Za-z0-9_.@/+-]+\.(?:sh|py|mjs|js)")

# A path in command position in a shell line: after an interpreter, or run
# directly. Environment assignments and flags may sit in between.
SHELL_CALL = re.compile(
    r"(?:^|[;&|(`]|\$\(|\s)(?:[A-Z_][A-Z0-9_]*=\S*\s+)*"
    r"(?:(?:bash|sh|python3?|node|zsh)\s+(?:-\S+\s+)*)?"
    r"[\"']?(?:\./|\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/)?"
    r"((?:scripts|benchmarks|crates|tools)/[A-Za-z0-9_.@/+-]+\.(?:sh|py|mjs|js))"
)

# The four helper names, so a line that CALLS a gate is never read as a command.
# `crate_gate errtests pdfluent cargo test -q -p pdfluent` is a landing-lane
# gate; read as shell it is a cargo invocation, and it is not one.
HELPER_CALL = re.compile(r"^\s*(?:run|zwaar|crate_gate|scoped)\s")


class Finding:
    def __init__(self, gate: str, path: str, line: int, what: str, via: list[str]) -> None:
        self.gate, self.path, self.line, self.what, self.via = gate, path, line, what, via

    def __str__(self) -> str:
        chain = " -> ".join(self.via) if self.via else "the gate's own command line"
        return (f"  {self.gate}: {self.path}:{self.line}\n"
                f"      {self.what}\n"
                f"      reached by: {chain}")


def strip_shell_comment(line: str) -> str:
    """Everything before an unquoted `#`.

    Quoting is counted rather than guessed: `grep '#.*'` is a command and
    `# cargo build` is prose, and a scan that could not tell them apart would
    have to choose between missing compiles and reporting comments.
    """
    out: list[str] = []
    quote = ""
    for i, ch in enumerate(line):
        if quote:
            out.append(ch)
            if ch == quote:
                quote = ""
            continue
        if ch in "'\"":
            quote = ch
            out.append(ch)
            continue
        if ch == "#" and (i == 0 or line[i - 1] in " \t;&|("):
            break
        out.append(ch)
    return "".join(out)


def cargo_in_shell(text: str) -> list[tuple[int, str]]:
    hits = []
    for number, raw in enumerate(text.splitlines(), 1):
        if HELPER_CALL.match(raw):
            continue
        line = strip_shell_comment(raw)
        for match in re.finditer(r"(?:^|[;&|(`]|\$\(|\s)cargo\s+(?:\+\S+\s+)*([a-z][a-z-]*)", line):
            if match.group(1) in COMPILING:
                hits.append((number, line.strip()[:100]))
                break
    return hits


def shell_children(text: str) -> set[str]:
    found = set()
    for raw in text.splitlines():
        if HELPER_CALL.match(raw):
            continue
        line = strip_shell_comment(raw)
        for match in SHELL_CALL.finditer(line):
            found.add(match.group(1))
    return found


def _fold(node: ast.AST, names: dict[str, str]) -> str:
    """The path a module-level constant stands for, as far as it can be read.

    `BACKUP = CORPUS / "backup-golden.sh"` is how half this directory names the
    script it drives, and the argument the subprocess call receives is
    `str(BACKUP)`. Without this, every one of those calls would be a child this
    scan could not follow -- which is the shape of a guard that is green because
    it stopped looking.

    Roots (`ROOT`, `WORTEL`, `pathlib.Path(__file__)`) fold to the empty string,
    so what comes out is repository-relative by construction.
    """
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.Name):
        return names.get(node.id, "")
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Div):
        left = _fold(node.left, names)
        right = _fold(node.right, names)
        return f"{left}/{right}" if left and right else (left or right)
    if isinstance(node, ast.Call):
        func = getattr(node.func, "attr", None) or getattr(node.func, "id", None)
        if func == "str" and node.args:
            return _fold(node.args[0], names)
    if isinstance(node, ast.Attribute):
        return ""
    return ""


def python_constants(tree: ast.Module) -> dict[str, str]:
    names: dict[str, str] = {}
    for node in tree.body:
        if not isinstance(node, ast.Assign) or len(node.targets) != 1:
            continue
        target = node.targets[0]
        if not isinstance(target, ast.Name):
            continue
        folded = _fold(node.value, names).strip("/")
        if folded:
            names[target.id] = folded
    return names


def _argv(node: ast.Call, names: dict[str, str]) -> list[str]:
    if not node.args:
        return []
    first = node.args[0]
    if isinstance(first, (ast.List, ast.Tuple)):
        return [_fold(element, names) or "?" for element in first.elts]
    if isinstance(first, ast.Constant) and isinstance(first.value, str):
        return first.value.split()
    return []


def scan_python(text: str) -> tuple[list[tuple[int, str]], set[str]]:
    try:
        tree = ast.parse(text)
    except SyntaxError:
        # Not silence: a file that does not parse is reported by the caller as a
        # finding of its own, because a guard that shrugged at it would be a
        # guard that can be switched off with a typo.
        raise
    names = python_constants(tree)
    hits: list[tuple[int, str]] = []
    children: set[str] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        func = getattr(node.func, "attr", None) or getattr(node.func, "id", None)
        if func not in {"run", "call", "check_call", "check_output", "Popen"}:
            continue
        argv = _argv(node, names)
        if not argv:
            continue
        head = pathlib.PurePath(argv[0]).name
        if head == "cargo" and len(argv) > 1 and argv[1] in COMPILING:
            hits.append((node.lineno, " ".join(argv[:5])))
        # sys.executable folds to "", and a call whose argv[0] this scan could
        # not read is treated as an interpreter for the same reason: the paths
        # after it are being RUN by something, and which something it is does
        # not change that.
        if head in INTERPRETERS or argv[0] in ("", "?"):
            for element in argv[1:]:
                for match in PATH_IN_TEXT.finditer(element):
                    children.add(match.group(0))
    return hits, children


def walk(gate: str, path: str, depth: int, seen: set[str], out: list[Finding], chain: list[str]) -> None:
    if depth > 3 or path in seen:
        return
    seen.add(path)
    # The gate script itself is never followed. Three guards drive it -- the lock
    # test starts a real `--fast` run behind a held lock to prove it waits -- and
    # following it would attribute every lane in the file to whichever guard
    # opened it. What the gate script's own lanes contain is the question this
    # file answers; it cannot also be evidence.
    if path == "scripts/ci/local_ci_gate.sh":
        return
    file = ROOT / path
    if not file.is_file():
        return
    text = file.read_text(errors="replace")
    here = chain + [path]
    if path.endswith(".py"):
        try:
            hits, children = scan_python(text)
        except SyntaxError as bad:
            out.append(Finding(gate, path, bad.lineno or 0,
                               f"this file does not parse, so nothing could be read from it: {bad.msg}",
                               chain))
            return
    else:
        hits, children = cargo_in_shell(text), shell_children(text)
    for line, what in hits:
        out.append(Finding(gate, path, line, what, chain))
    for child in sorted(children):
        walk(gate, child, depth + 1, seen, out, here)


def fast_lane_gates(text: str) -> list[tuple[str, str]]:
    gates = []
    for raw in text.splitlines():
        match = re.match(r"\s*run\s+(\S+)\s+(.*\S)\s*$", raw)
        if not match:
            continue
        name, command = match.group(1), match.group(2)
        if name.startswith("$"):
            continue
        gates.append((name, command))
    return gates


def helpers_still_defer(text: str) -> list[str]:
    """Each deferring helper's body still says DEFERRED.

    Bodies are read between `name() {` and the closing brace in column one,
    which is how every helper in that file is written.
    """
    complaints = []
    for helper, word in DEFERRING.items():
        body = re.search(rf"^{helper}\(\)\s*\{{(.*?)^\}}", text, re.S | re.M)
        if not body:
            complaints.append(f"local_ci_gate.sh no longer defines a `{helper}` helper, "
                              f"so this guard cannot tell which gates a push runs")
        elif word not in body.group(1):
            complaints.append(f"`{helper}` in local_ci_gate.sh no longer says {word}: "
                              f"if it stopped deferring, every gate it carries is on every push")
    return complaints


def main() -> int:
    if not GATE.is_file():
        print(f"{GATE} is not there; nothing could be checked.")
        return 2
    text = GATE.read_text(errors="replace")

    broken = helpers_still_defer(text)
    if broken:
        for line in broken:
            print(f"a_push_gate_does_not_compile_the_workspace: {line}")
        return 2

    gates = fast_lane_gates(text)
    if len(gates) < MIN_GATES:
        print(f"a_push_gate_does_not_compile_the_workspace: only {len(gates)} gates found on the "
              f"push lane, and there should be at least {MIN_GATES}.")
        print("  The scan is reading something other than the gate list. A scan that finds "
              "nothing approves everything, so this is a refusal and not a pass.")
        return 2

    findings: list[Finding] = []
    for name, command in gates:
        words = command.split()
        if words and pathlib.PurePath(words[0]).name == "cargo" and len(words) > 1 and words[1] in COMPILING:
            findings.append(Finding(name, "scripts/ci/local_ci_gate.sh", 0, command, []))
        seen: set[str] = set()
        for match in SHELL_CALL.finditer(" " + command):
            walk(name, match.group(1), 1, seen, findings, [])

    if findings:
        print(f"a_push_gate_does_not_compile_the_workspace: {len(findings)} compiling step(s) "
              f"on the push lane, over {len(gates)} gates.")
        for finding in findings:
            print(finding)
        print("")
        print("A push does not need this: the landing lane compiles and tests the crates a")
        print("landing touches, and master takes only fast-forwards, so what passes there is")
        print("what lands. Move the gate to `zwaar` (landing and full lanes) or to")
        print("`crate_gate <crate>` (landing lane, when that crate is touched) in")
        print("scripts/ci/local_ci_gate.sh. Raising nothing, skipping nothing: the gate keeps")
        print("its job, on the lane that was already paying for a compiler.")
        return 1

    print(f"a_push_gate_does_not_compile_the_workspace: OK — {len(gates)} gates on the push "
          f"lane, none of them reaches a compiler.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
