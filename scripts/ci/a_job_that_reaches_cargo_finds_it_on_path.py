#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job that reaches cargo says where cargo is -- including through a script. (#516)

WHAT HAPPENED

`publication-guards.yml` failed on every push to master from 09-09-2026: 38 runs,
none green, all of them on

    FileNotFoundError: [Errno 2] No such file or directory: 'cargo'

and next to it, on the same runner and out of the same script,
`ci.yml`'s `public-snapshot-build` died in nine seconds the same way. These are
the guards for the published tree and the licence texts, so the thing that was
not being checked was what a stranger receives.

WHY NOBODY SAW IT COMING, WHICH IS THE POINT OF THIS FILE

The word `cargo` appears nowhere in either job. The step reads

    run: bash scripts/ci/public_snapshot_audit.sh

and that script runs `simulate_public_tree.py`, and THAT shells out to
`cargo metadata` on the tree it has just assembled. A guard that reads only the
workflow sees a Python file scan; the compiler is two hops down. So this one
follows the scripts, the way `a_push_gate_does_not_compile_the_workspace.py`
follows them to answer the neighbouring question, and reuses that file's reader
rather than growing a second one that would drift.

WHICH MACHINES THIS IS ABOUT

Not a hosted runner: `ubuntu-latest` ships a toolchain on the PATH of every
step, and demanding a step there would be noise. It is about a runner we own --
the persistent desktop, and the instances a wave buys. The desktop is a service,
and a service reads no login profile, so the directory rustup installed cargo
into is not on the PATH an Actions `run:` step gets, while the same command
works for the person who set the machine up. That difference has now cost three
incidents: #292, #495 and #516.

WHAT SATISFIES IT

Either a toolchain action, or a step that puts cargo where the shell can find it
and CHECKS that it did -- `scripts/ci/cargo_on_path.sh` is that step. Before the
first step that needs it, because a cache action that cannot run `rustc -vV` has
already failed the job by the time a later step repairs the PATH.

`a_wave_job_that_compiles_carries_its_toolchain.py` asks the stricter question of
the jobs on a bought instance: there the image is the unknown, and installing a
toolchain is the answer rather than trusting a PATH. This file asks the weaker
question of every machine we own, and finds the case that one cannot see: a need
that only exists inside a script.

A runs-on routed through the runner_pool input (#636) is judged per branch: a
need that runs only on the github half is answered by the hosted image, and any
other need by a provider on the self half -- the shared setup action counts
when the job asked it for rust. The two halves partition every run, so a job
that provides on one side only is still a job that provides everywhere. Where
the workflow counts a push as self (release-smoke.yml, whose tag trigger would
otherwise spend hosted minutes), the push-or-self provider and the
not-push-and-not-self need partition the same way and are judged the same.

Exit codes:
  0  every such job can find cargo before the first step that needs it
  1  one cannot, or the shared step has stopped checking anything
  2  nothing could be read: no workflow directory, or so few jobs that the scan
     is not looking at what it thinks it is
"""
from __future__ import annotations

import argparse
import ast
import pathlib
import re
import sys

try:
    import yaml
except ImportError:  # pragma: no cover - the lane installs pyyaml
    print("a_job_that_reaches_cargo_finds_it_on_path: pyyaml is not installed, so no "
          "workflow could be read.", file=sys.stderr)
    sys.exit(2)

ROOT = pathlib.Path(__file__).resolve().parents[2]

# The reader that already knows how to follow a command line into the scripts it
# runs: which paths are in command position, which are merely mentioned, and how
# to fold `BACKUP = CORPUS / "backup-golden.sh"` back into a path. Its own
# question is which gates compile; the walking is the same walking, and two
# copies of it would answer differently within a month.
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import a_push_gate_does_not_compile_the_workspace as reader  # noqa: E402

# The runner expressions that mean a machine we own: the persistent desktop, and
# the instance a wave bought.
#
# `xfa-fast` and `xfa-corpus` are here for the reason orchestration_stays_hosted
# .py records: `self-hosted` is implied by a custom label but does not have to be
# written, and `runs-on: [xfa-fast]` routes to the same desktop while containing
# neither word. (codex, #1875)
OWN_MACHINE = re.compile(
    r"self-hosted|xfa-fast|xfa-corpus|inputs\.runner_label|create-runner\.outputs\.label")

# In command position, not merely on the line. `pgrep -x cargo` is an argument
# and `[[ -e "$t/.cargo-lock" ]]` is a filename; both were read as invocations
# by the first version of this scan.
# Where one command ends and the next begins, in a shell line -- outside quotes.
# `echo "note; cargo metadata is needed"` is one command and a sentence, and
# splitting inside the quotes turned the sentence into a second command that
# ran cargo. A guard that invents a need is worse than one that misses it: it
# refuses an honest push. (codex, #1875)
SEPARATORS = ";&|(`"


def split_commands(line: str) -> list[str]:
    """The commands on this line, with quoted text left whole."""
    parts, current, quote = [], [], ""
    for character in line:
        if quote:
            current.append(character)
            if character == quote:
                quote = ""
            continue
        if character in "'\"":
            quote = character
            current.append(character)
            continue
        if character in SEPARATORS:
            parts.append("".join(current))
            current = []
            continue
        current.append(character)
    parts.append("".join(current))
    return parts

# Words that may stand in front of the binary and still leave it the command
# being run. `env RUSTFLAGS=-Dwarnings cargo check`, `sudo -u runner cargo
# check`, `time cargo test`, `exec cargo build` -- all of them run cargo, and a
# pattern that allowed only bare assignments read them as something else.
# (codex, #1875)
WRAPPERS = {"sudo", "env", "command", "exec", "time", "nice", "ionice", "stdbuf",
            "nohup", "setsid"}

# The words a shell puts in front of a command without changing which command it
# is. `if ! cargo package ...; then` is how this repository asks cargo a
# question, and reading `if` as the program meant the answer was no. (codex,
# #1875)
CONTROL = {"if", "elif", "while", "until", "then", "else", "do", "!", "{", "("}
ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


def rust_command(part: str) -> str:
    """The rust binary this command runs, or the empty string.

    Written as a walk over the words rather than one more alternative in a
    regular expression: the prefixes stack (`sudo -u runner env FOO=1 cargo`),
    and each new spelling was costing a round of review.
    """
    words = part.split()
    while words:
        head = words[0]
        if ASSIGNMENT.match(head) or head in CONTROL:
            words = words[1:]
            continue
        if head in WRAPPERS:
            words = words[1:]
            # `sudo -u runner`, `env -i`: flags of the wrapper, and the value
            # that some of them take.
            while words and words[0].startswith("-"):
                flag = words.pop(0)
                if flag in ("-u", "-g", "-C", "--user", "--group", "--chdir") and words:
                    words.pop(0)
            continue
        return head if pathlib.PurePath(head.strip("\"'")).name in RUST_BINARIES else ""
    return ""


INTERPRETERS = ("bash", "sh", "zsh", "python", "python3")


def script_run(part: str) -> str:
    """The script this command RUNS, or the empty string.

    `test -f scripts/ci/cargo_on_path.sh` names the helper in an argument and
    changes no PATH, and the looser reader next door -- which is allowed to
    follow a path it is unsure about -- read that as running it. Here the
    question is whether the repair happened, so only the command position
    counts. (codex, #1875)
    """
    words = part.split()
    while words:
        head = words[0]
        if head in CONTROL:
            words = words[1:]
            continue
        if ASSIGNMENT.match(head) or head in WRAPPERS:
            words = words[1:]
            while words and words[0].startswith("-"):
                flag = words.pop(0)
                if flag in ("-u", "-g", "-C", "--user", "--group", "--chdir") and words:
                    words.pop(0)
            continue
        break
    if not words:
        return ""
    head = words[0].strip("\"'")
    if pathlib.PurePath(head).name in INTERPRETERS:
        for word in words[1:]:
            if not word.startswith("-"):
                return word.strip("\"'").lstrip("./")
        return ""
    return head.lstrip("./") if "/" in head else ""


# A command substitution is a command, quotes or no quotes: `version="$(cargo
# metadata)"` runs cargo inside a double-quoted string, where the splitter above
# rightly sees one word. Read separately rather than by teaching the splitter
# about `$(`, because the splitter's job is to keep a SENTENCE whole and this
# one's is to open the one thing inside a sentence that still runs. (codex,
# #1875)
SUBSTITUTION = re.compile(r"\$\(([^()]*)\)|`([^`]*)`")


def reaches_rust(line: str) -> str:
    """The rust invocation in this shell line, or the empty string."""
    for part in split_commands(line):
        found = rust_command(part)
        if found:
            return found
    for outer, inner in SUBSTITUTION.findall(line):
        for part in split_commands(outer or inner):
            found = rust_command(part)
            if found:
                return found
    return ""

# Actions that run the compiler before any step of ours does. rust-cache runs
# `rustc -vV` to build its cache key, which is the line enterprise-acceptance
# failed on in wave 34365955374 (#495).
ACTIONS_NEEDING_RUST = ("Swatinem/rust-cache",)

TOOLCHAIN_ACTIONS = ("dtolnay/rust-toolchain", "actions-rs/toolchain")

# The shared setup the release workflows call on the self-hosted branch instead
# of the setup-* actions (#636): it installs rustup and appends ~/.cargo/bin to
# $GITHUB_PATH, so it is a provider -- but only for the tools the job actually
# asked for. A call with `tools: node,python` installs no Rust and must not
# count; the package job learned this the week the routing landed.
SELFHOSTED_SETUP = "./.github/actions/setup-selfhosted"

# The two halves of a runs-on routed through the runner_pool input, in the
# spelling normal_condition compares in. They partition every run: the github
# half lands on a hosted image, which ships cargo, so only the self half needs
# a provider the guard can see.
#
# Each half has the push-counts-as-self spelling too (release-smoke.yml): the
# tag push carries no dispatch input, so the pool test reads push-or-self and
# the hosted-only side its negation. Both spellings partition the same way --
# the provider runs on every self-machine run, the need only where the image
# ships the toolchain -- so both are accepted, in either term order.
SELF_HALVES = {
    "inputs.runner_pool=='self'",
    "github.event_name=='push'||inputs.runner_pool=='self'",
    "inputs.runner_pool=='self'||github.event_name=='push'",
}
GITHUB_HALVES = {
    "inputs.runner_pool!='self'",
    "github.event_name!='push'&&inputs.runner_pool!='self'",
    "inputs.runner_pool!='self'&&github.event_name!='push'",
}

# The hosted images the github half may land on. The routed rule trusts the
# image only when the workflow names one where the guard can read it.
HOSTED_RUNNERS = ("ubuntu-", "macos-", "windows-")

# Written as a set and never as a list, so that this file does not read itself
# as a job that runs cargo. It did, twice, while it was being written.
RUST_BINARIES = {"cargo", "rustc", "rustup"}

# The vocabulary of the repair. A segment of a command line that says one of
# these is the job putting cargo within reach, not the job needing it -- but
# saying them is not the same as having done it, which is what makes the two
# constants below different from each other.
REPAIR_WORDS = ("scripts/ci/cargo_on_path.sh", ".cargo/bin", ".cargo/env",
                "rustup which cargo")

# WHAT ACTUALLY MAKES CARGO FINDABLE FOR THE STEPS THAT FOLLOW.
#
# `rustup which cargo` prints a path and changes nothing; `export PATH=...` in a
# `run:` block dies with that block's shell. The one thing a step can do that
# outlives it is write the directory to $GITHUB_PATH, which is what
# scripts/ci/cargo_on_path.sh does and what ci-ephemeral's toolchain step does
# by hand. Accepting the printing on its own would have let through exactly the
# failure this guard exists to prevent. (codex, #1875)
SHARED_STEP = "scripts/ci/cargo_on_path.sh"

# What the shared step must still do. A file that has been emptied satisfies
# every job that calls it and checks nothing.
SHARED_STEP_MUST_SAY = ("command -v cargo", "GITHUB_PATH", "cargo --version")

# HOW OFTEN A CONDITION LETS A STEP RUN, as an order rather than a flag.
#
# A provider is only a provider for a step it is guaranteed to run before. A flat
# "these four are unconditional" set said that a step with no `if:` covers a step
# with `if: always()`, and it does not: after any earlier failure the first is
# skipped and the second still runs, which is the shape half the jobs in this
# repository are written in. So: a provider covers a need when it runs at least
# as often, or when the two carry the same condition verbatim. (codex, #1875)
RUNS_AS_OFTEN_AS = {"": 0, "success()": 0, "!cancelled()": 1, "! cancelled()": 1,
                    "always()": 2}

def need_rank(need: str) -> int:
    """How often a step under this condition runs, on the same scale.

    `always()` and `failure()` are the two functions that let a step run after
    something above it went wrong, and `cancelled()` the one that lets it run
    after a cancellation. Everything else -- including an ordinary predicate
    like `steps.x.outputs.go == 'true'` -- is implicitly `success() && ...`,
    which GitHub applies for you, so a step with no condition of its own runs
    whenever such a step does. (codex, #1875)
    """
    plain = need.replace(" ", "")
    if "always()" in plain or "failure()" in plain:
        return 2
    if "!cancelled()" in plain:
        return 1
    if "cancelled()" in plain:
        # `cancelled()` on its own is the opposite of `!cancelled()`: the step
        # runs ONLY when the job was cancelled, which is precisely when a
        # `!cancelled()` provider did not. Only `always()` covers it, so it
        # ranks with the conditions that survive a failure. (codex, #1875)
        return 2
    return 0


def covers(provider: str, need: str) -> bool:
    """Whether a step under `provider` runs whenever a step under `need` does.

    A provider spelled one of the four house conditions is compared by how often
    it runs -- and an ordinary predicate on the need does not raise that bar,
    because such a step is success-gated too.

    A provider carrying an arbitrary predicate of its own is a different matter:
    nothing here can tell when it is true, so it covers only a need that carries
    the same predicate, or one built from it with `&&`. That last case is
    ci.yml's guard job, where the need is `!cancelled() && steps.fixture-diff
    .outputs.moved == 'true'` under a provider that carries `!cancelled()`.
    """
    if provider == need:
        return True
    if provider not in RUNS_AS_OFTEN_AS:
        # Only over an `&&` chain. `A && B || C` runs through C with A false, so
        # finding the provider's condition among the terms proves nothing there.
        # (codex, #1875)
        return bool(provider) and "||" not in need and provider in [
            part.strip() for part in need.split("&&")]
    return RUNS_AS_OFTEN_AS[provider] >= need_rank(need)


# How deep a script chain is followed. Three hops is what the neighbouring guard
# uses, and #516 was two.
MAX_HOPS = 3

# A scan that finds nothing approves everything.
MIN_JOBS = 8


def cargo_in_shell(text: str) -> list[tuple[int, str]]:
    """Lines of a shell script that invoke cargo, rustc or rustup."""
    hits = []
    for number, raw in enumerate(text.splitlines(), 1):
        if reader.HELPER_CALL.match(raw):
            continue
        line = reader.strip_shell_comment(raw)
        if reaches_rust(line):
            hits.append((number, line.strip()[:100]))
    return hits


RUNNERS = {"run", "call", "check_call", "check_output", "Popen"}


def _rust_argv(node: ast.AST) -> list[str] | None:
    """The words of a sequence literal that starts with cargo, rustc or rustup."""
    if not isinstance(node, (ast.List, ast.Tuple)) or not node.elts:
        return None
    first = node.elts[0]
    if not (isinstance(first, ast.Constant) and first.value in RUST_BINARIES):
        return None
    return [e.value for e in node.elts[:4]
            if isinstance(e, ast.Constant) and isinstance(e.value, str)]


def _names_holding_argv(tree: ast.Module) -> dict[str, tuple[int, list[str]]]:
    """Variables that hold a cargo argv, directly or inside a container.

    `simulate_public_tree.py` -- the file this whole issue is about -- writes

        stappen = {"metadata": ["cargo", "metadata", "--no-deps"], ...}
        subprocess.run(stappen[stap], cwd=tmp, env=env)

    so the literal and the call are ten lines apart with a dictionary in
    between. Following the name is what makes that case visible; it is also
    what keeps an unrelated `EXPECTED = ["cargo", "metadata"]` in a fixture from
    being read as an invocation, because nothing ever runs it. (codex, #1875)
    """
    holders: dict[str, tuple[int, list[str]]] = {}
    for node in ast.walk(tree):
        target = None
        if isinstance(node, ast.Assign) and len(node.targets) == 1:
            target = node.targets[0]
        elif isinstance(node, ast.AnnAssign):
            target = node.target
        if not isinstance(target, ast.Name):
            continue
        for inner in ast.walk(node.value) if node.value is not None else []:
            words = _rust_argv(inner)
            if words:
                holders[target.id] = (inner.lineno, words)
                break
    return holders


def cargo_in_python(text: str) -> list[tuple[int, str]]:
    """Places a Python file hands `cargo` to a subprocess.

    Read as a syntax tree and not as text, for the reason the neighbouring guard
    records: half the guards in this directory carry `cargo test` inside a
    fixture string or an error message, and a scan that read those would be red
    about documentation.

    ONLY WHAT IS RUN. The first version counted every list literal whose head
    was `cargo`, wherever it stood, and that is two mistakes at once: a fixture
    holding `EXPECTED = ["cargo", "metadata"]` would have made the guard demand
    a toolchain for a job that never compiles, and this very file -- which has
    to write the three names down to compare against them -- was reported twice
    while it was being written. So a literal counts when it reaches a subprocess
    call: at the call site, or through a name assigned it, however deeply that
    name wraps it. (codex, #1875)
    """
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return []
    holders = _names_holding_argv(tree)
    hits: list[tuple[int, str]] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        name = getattr(node.func, "attr", None) or getattr(node.func, "id", None)
        if name not in RUNNERS:
            continue
        # `subprocess.run(args=["cargo", "metadata"])` is the same call written
        # with a keyword, and reading only the positional argument skipped it.
        # (codex, #1875)
        argv = node.args[0] if node.args else next(
            (kw.value for kw in node.keywords if kw.arg == "args"), None)
        if argv is None:
            continue
        # `subprocess.run(["cargo", ...])` and `subprocess.run(("cargo", ...))`:
        # a tuple is as runnable as a list, and reading only lists here let the
        # tuple spelling through. (codex, #1875)
        words = _rust_argv(argv)
        if words:
            hits.append((argv.lineno, " ".join(words)))
            continue
        # `subprocess.run("cargo build", shell=True)`.
        if isinstance(argv, ast.Constant) and isinstance(argv.value, str):
            head = argv.value.split()[:1]
            if head and head[0] in RUST_BINARIES:
                hits.append((node.lineno, argv.value[:80]))
            continue
        # `run(command)` and `run(steps[name])`: the argv was built earlier.
        holder = argv if isinstance(argv, ast.Name) else getattr(argv, "value", None)
        if isinstance(holder, ast.Name) and holder.id in holders:
            line, built = holders[holder.id]
            hits.append((line, " ".join(built)))
    return hits


def keyword_children(text: str) -> set[str]:
    """Scripts named in a subprocess call's `args=` keyword."""
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return set()
    found: set[str] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        name = getattr(node.func, "attr", None) or getattr(node.func, "id", None)
        if name not in RUNNERS:
            continue
        argv = next((kw.value for kw in node.keywords if kw.arg == "args"), None)
        if not isinstance(argv, (ast.List, ast.Tuple)):
            continue
        for element in argv.elts:
            if isinstance(element, ast.Constant) and isinstance(element.value, str):
                for match in reader.PATH_IN_TEXT.finditer(element.value):
                    found.add(match.group(0))
    return found


def walk(root: pathlib.Path, path: str, depth: int, seen: dict[str, int],
         chain: list[str]) -> tuple[str, list[str] ] | None:
    """The first cargo invocation reachable from this script, with the way there.

    `seen` remembers the DEPTH a script was reached at, not merely that it was.
    A helper reached first down a long path, where the hop budget ran out before
    its own children could be read, would otherwise be skipped when the same
    step reaches it directly a moment later -- and the compiler behind it would
    be invisible for no better reason than the order the paths were read in.
    (codex, #1875)
    """
    if depth > MAX_HOPS or seen.get(path, MAX_HOPS + 1) <= depth:
        return None
    seen[path] = depth
    # The gate script is never followed: it holds every gate in the repository,
    # and a job that merely reads it is not running them. Same rule, same reason
    # as the neighbouring guard.
    if path == "scripts/ci/local_ci_gate.sh":
        return None
    file = root / path
    if not file.is_file():
        return None
    text = file.read_text(errors="replace")
    if path.endswith(".py"):
        hits = cargo_in_python(text)
        try:
            _, children = reader.scan_python(text)
        except SyntaxError:
            children = set()
        # The reader next door finds a child script only in a positional argv,
        # so `subprocess.run(args=[sys.executable, "scripts/ci/inner.py"])`
        # hid whatever inner.py runs. Same keyword, same answer, here.
        # (codex, #1875)
        children |= keyword_children(text)
    else:
        hits = cargo_in_shell(text)
        children = reader.shell_children(text)
    here = chain + [path]
    if hits:
        line, what = hits[0]
        return f"{path}:{line}  {what}", here
    for child in sorted(children):
        found = walk(root, child, depth + 1, seen, here)
        if found:
            return found
    return None


def needs_cargo(root: pathlib.Path, step: dict) -> str:
    """Why this step needs cargo, or the empty string.

    Read segment by segment, because a step may repair the PATH and use it in
    the same breath: `bash scripts/ci/cargo_on_path.sh && cargo metadata`. The
    first version dropped the whole step the moment it recognised the repair,
    and that hid a real invocation -- one that can still fail, because the
    script runs in a child shell whose `export PATH` the parent never sees and
    $GITHUB_PATH reaches only the steps that come after. So the repair's own
    segment is dropped and the rest of the line is read. (codex, #1875)
    """
    uses = str(step.get("uses", ""))
    for action in ACTIONS_NEEDING_RUST:
        if uses.startswith(action):
            return f"uses: {uses}"
    command = step.get("run")
    if not isinstance(command, str):
        return ""
    segments = []
    for raw in command.splitlines():
        line = reader.strip_shell_comment(raw)
        for segment in split_commands(line):
            if not any(word in segment for word in REPAIR_WORDS):
                segments.append(segment)
    for segment in segments:
        if reaches_rust(segment):
            return f"run: {segment.strip()[:80]}"
    seen: dict[str, int] = {}
    for segment in segments:
        for match in reader.SHELL_CALL.finditer(" " + segment):
            found = walk(root, match.group(1), 1, seen, [])
            if found:
                where, chain = found
                # The chain already ends at the file the hit is in, and `where`
                # names that file with its line: printing both put it twice.
                hops = " -> ".join(chain[1:-1] + [where])
                return f"run: {segment.strip()[:60]} -> {hops}"
    return ""


def condition(step: dict) -> str:
    """A step's `if:`, stripped of the expression braces that do not change it."""
    text = str(step.get("if", "")).strip()
    if text.startswith("${{") and text.endswith("}}"):
        text = text[3:-2].strip()
    return text


def normal_condition(step: dict) -> str:
    """The step's `if:` down to the spelling the two halves compare in."""
    return condition(step).replace(" ", "").replace('"', "'")


# A matrix key the runs-on expression indexes: `matrix.os-self`, `matrix.os`.
_MATRIX_REF = re.compile(r"matrix\.([A-Za-z0-9_-]+)")


def _matrix_values(matrix: object, key: str) -> list[str]:
    """Every value a matrix holds under this key, as text."""
    if not isinstance(matrix, dict):
        return []
    found: list[str] = []
    value = matrix.get(key)
    if isinstance(value, (str, list)):
        found.append(str(value))
    include = matrix.get("include") or []
    if isinstance(include, list):
        for entry in include:
            if isinstance(entry, dict):
                value = entry.get(key)
                if isinstance(value, (str, list)):
                    found.append(str(value))
    return found


def routed_through_runner_pool(job: dict) -> bool:
    """Whether this job's runs-on routes across the two pools (#636).

    The github half then lands on a hosted image, which ships cargo -- but the
    guard trusts that only when the workflow names the image where it can read
    it: in the expression itself (`|| 'ubuntu-latest'`), or in the matrix key
    the github branch indexes. Every leg must say so, not one: a single
    unhosted leg is the job the guard exists to catch.
    """
    runs_on = str(job.get("runs-on", ""))
    if "runner_pool" not in runs_on:
        return False
    refs = _MATRIX_REF.findall(runs_on)
    matrix = (job.get("strategy") or {}).get("matrix") or {}
    self_keys = [ref for ref in refs if ref.endswith("-self")]
    github_keys = [ref for ref in refs if not ref.endswith("-self")]
    self_values = [value for key in self_keys for value in _matrix_values(matrix, key)]
    github_values = [
        value for key in github_keys for value in _matrix_values(matrix, key)
    ]
    self_ok = "self-hosted" in runs_on or (
        bool(self_values) and all("self-hosted" in value for value in self_values)
    )
    github_ok = any(label in runs_on for label in HOSTED_RUNNERS) or (
        bool(github_values)
        and all(
            any(label in value for label in HOSTED_RUNNERS) for value in github_values
        )
    )
    return self_ok and github_ok


def finds_cargo(step: dict) -> bool:
    """Whether this step makes cargo findable for the steps that follow it."""
    # `continue-on-error: true` means the job goes on when this step fails, and
    # the one way this step fails is the one thing it is here to prevent: no
    # cargo. A step allowed to fail is not a guarantee. (codex, #1875)
    if str(step.get("continue-on-error", "")).lower() in ("true", "${{ true }}"):
        return False
    uses = str(step.get("uses", ""))
    if any(uses.startswith(action) for action in TOOLCHAIN_ACTIONS):
        return True
    if uses.strip() == SELFHOSTED_SETUP:
        tools = str((step.get("with") or {}).get("tools", ""))
        return "rust" in {token.strip() for token in tools.split(",")}
    command = step.get("run")
    if not isinstance(command, str):
        return False
    # WHAT IT RUNS, not what it says -- the same rule as the shared step below.
    # A step whose only mention of the repair is `# bash scripts/ci/
    # cargo_on_path.sh` counts as the repair otherwise, and a later real cargo
    # call is approved on the strength of a comment. (codex, #1875)
    command = "\n".join(reader.strip_shell_comment(line)
                        for line in command.splitlines())
    # RUN, not mentioned: `test -f scripts/ci/cargo_on_path.sh` names the helper
    # and changes no PATH. The same reader that decides which scripts a step
    # runs decides it here. (codex, #1875)
    runs = {script_run(part) for line in command.splitlines()
            for part in split_commands(line)}
    if SHARED_STEP in runs:
        return True
    return "GITHUB_PATH" in command and any(word in command for word in REPAIR_WORDS)


def our_jobs(workflows: pathlib.Path):
    """Every (file, job name, job) that runs on a machine we own."""
    for path in sorted(workflows.glob("*.yml")) + sorted(workflows.glob("*.yaml")):
        try:
            document = yaml.safe_load(path.read_text(errors="replace"))
        except yaml.YAMLError as bad:
            yield path, "(unreadable)", {"__error__": str(bad)}
            continue
        if not isinstance(document, dict):
            continue
        jobs = document.get("jobs")
        if not isinstance(jobs, dict):
            continue
        for name, job in jobs.items():
            if not isinstance(job, dict):
                continue
            runs_on = str(job.get("runs-on", ""))
            # A runs-on routed through runner_pool is ours too: dispatch can
            # send it to the self half. The expression spells no literal
            # self-hosted -- the label lives in the matrix it indexes.
            if OWN_MACHINE.search(runs_on) or "runner_pool" in runs_on:
                yield path, str(name), job


def shared_step(root: pathlib.Path) -> list[str]:
    """The step three jobs now share, checked where it lives.

    A job that calls a file which no longer checks anything is a job with a
    green tick over an unanswered question -- which is the whole of #290 and
    half of why this repository has the guards it has.
    """
    file = root / SHARED_STEP
    if not file.is_file():
        return [f"{SHARED_STEP} is not there, and three jobs call it (#516)"]
    # WHAT IT RUNS, not what it says. A script whose three phrases are all in
    # comments -- above an `exit 0` -- would satisfy this check while every
    # caller lost both the PATH and the check. (codex, #1875)
    text = "\n".join(reader.strip_shell_comment(line)
                     for line in file.read_text(errors="replace").splitlines())
    missing = [phrase for phrase in SHARED_STEP_MUST_SAY if phrase not in text]
    if missing:
        return [f"{SHARED_STEP} no longer contains {', '.join(repr(m) for m in missing)}, "
                "so a job calling it can still meet the FileNotFoundError it exists "
                "to prevent (#516)"]
    return []


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(ROOT),
                        help="the checkout to read (a fixture tree, in the test)")
    parser.add_argument("--skip-shared-step", action="store_true",
                        help="only judge the workflows, for a fixture that has no scripts/ci")
    arguments = parser.parse_args()
    root = pathlib.Path(arguments.repo).resolve()
    workflows = root / ".github" / "workflows"
    if not workflows.is_dir():
        print(f"a_job_that_reaches_cargo_finds_it_on_path: no {workflows}; nothing was checked.")
        return 2

    findings: list[str] = []
    seen = 0
    for path, name, job in our_jobs(workflows):
        if "__error__" in job:
            findings.append(f"{path.name}: could not be parsed: {job['__error__']}")
            continue
        seen += 1
        steps = [s for s in (job.get("steps") or []) if isinstance(s, dict)]
        # EVERY STEP THAT NEEDS CARGO, not the first one. A provider and the
        # first need can share a condition while a later need carries another,
        # and stopping at the first would approve the job on the strength of a
        # step whose question was already answered. (codex, #1875)
        needs = [(index, why) for index, step in enumerate(steps)
                 if (why := needs_cargo(root, step))]
        if not needs:
            continue
        providers = [(index, condition(step)) for index, step in enumerate(steps)
                     if finds_cargo(step)]
        routed = routed_through_runner_pool(job)
        for index, why in needs:
            wanted = condition(steps[index])
            if any(where < index and covers(under, wanted) for where, under in providers):
                continue
            if routed:
                if normal_condition(steps[index]) in GITHUB_HALVES:
                    # Runs only where the image ships the toolchain.
                    continue
                if any(
                    where < index and normal_condition(steps[where]) in SELF_HALVES
                    for where, _ in providers
                ):
                    # The self half provides it; the github half is the image.
                    continue
            here = next((where for where, _ in providers if where == index), None)
            if here is not None:
                # $GITHUB_PATH reaches the steps AFTER this one, and the script's
                # own `export PATH` dies with the child shell it ran in. So a
                # step that repairs the PATH and then uses it has not been
                # helped by either.
                findings.append(
                    f"{path.name}: job `{name}` repairs the PATH and uses it in the same "
                    f"step {index + 1}: $GITHUB_PATH reaches the next step, not this "
                    f"one, and the script's own export dies with its shell.\n"
                    f"      that step: {why}")
                break
            skippable = next(((where, under) for where, under in providers
                              if where < index), None)
            if skippable is not None:
                where, under = skippable
                findings.append(
                    f"{path.name}: job `{name}` puts cargo on PATH at step {where + 1} "
                    f"only when `{under or 'no condition'}`, while step {index + 1} "
                    f"needs it under `{wanted or 'no condition'}`.\n"
                    f"      that step: {why}")
                break
            later = next((where for where, _ in providers if where > index), None)
            if later is not None:
                findings.append(
                    f"{path.name}: job `{name}` puts cargo on PATH at step {later + 1}, "
                    f"after step {index + 1} has already needed it.\n"
                    f"      that step: {why}")
                break
            findings.append(
                f"{path.name}: job `{name}` reaches cargo and never says where it is.\n"
                f"      step {index + 1} needs it: {why}")
            break

    if seen < MIN_JOBS:
        print(f"a_job_that_reaches_cargo_finds_it_on_path: only {seen} job(s) found on a machine "
              f"we own, and there should be at least {MIN_JOBS}.")
        print("  The scan is not reading the workflows it thinks it is. A scan that finds "
              "nothing approves everything.")
        return 2

    if not arguments.skip_shared_step:
        findings.extend(shared_step(root))

    if findings:
        print(f"a_job_that_reaches_cargo_finds_it_on_path: {len(findings)} problem(s) over "
              f"{seen} job(s) on a machine we own.")
        for finding in findings:
            print(f"  {finding}")
        print("")
        print("A runner we own is a service, and a service reads no login profile: the")
        print("directory rustup put cargo in is not on the PATH a `run:` step gets. Add")
        print("")
        print("      - name: Cargo on PATH")
        print("        run: bash scripts/ci/cargo_on_path.sh")
        print("")
        print("before the first step that needs it -- or, on a bought instance, install the")
        print("toolchain the way a_wave_job_that_compiles_carries_its_toolchain.py asks.")
        return 1

    print(f"a_job_that_reaches_cargo_finds_it_on_path: OK — {seen} job(s) on a machine we own, "
          f"every one that reaches cargo can find it.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
