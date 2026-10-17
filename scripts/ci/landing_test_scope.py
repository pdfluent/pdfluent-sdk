#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Which of the landing lane's script tests a change cannot reach.

    python3 scripts/ci/landing_test_scope.py            # names, space-separated
    python3 scripts/ci/landing_test_scope.py --explain  # and why, per test

WHY (COS, 28-09-2026). The landing of #2019, a change to two benchmark scripts,
held the landing lock for 68 minutes: `local_ci_gate.sh --landing` ran all 315
of its gates on the Mac, and 2,978 seconds of guard time went to tests of the
gate pool, the seed of the public repository, the gate lock and the remote gate,
none of which that change could have moved. The crates were already scoped to
the landing (#343); the tests of scripts were not.

WHAT IS SCOPED. A TEST of a script -- a gate whose entry is `test_*.py`,
`*_test.py`, `*.test.mjs` or a file under a `tests/` directory -- reads the
script it tests and the fixtures it builds, and nothing else can change its
verdict. So a test is out of a landing when the change touches none of its
INPUTS: its own file, and everything that file reaches, followed through
imports and through every path it names (`scripts/...`, `.githooks/...`,
`remote_gate.sh`), one file to the next. A test that runs `cargo` or reads a
crate reaches every crate, the manifests and the lock file.

WHAT IS NOT. Every other gate: a guard over the tree (formatting, licences, a
register, the public README) reads files nobody can list, and it keeps running.
So does every test when the change touches the gate itself, this file, the hook,
or anything it cannot read -- an unreadable diff, an unreadable test -- because
the answer to "I do not know" is to run it.

THE NET. What a landing skips here runs every night on master in the full lane
(`scripts/cos/nightly_full_gate.sh`), so a test broken by a change it could not
see turns that run red the next morning rather than never.

Exit 0 always with the names on stdout (none when everything runs); exit 2 when
the gate file itself cannot be read.
"""
from __future__ import annotations

import argparse
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
GATE = "scripts/ci/local_ci_gate.sh"
# A change to any of these runs every test: they decide what runs, or how.
EVERYTHING = (GATE, "scripts/ci/landing_test_scope.py", ".githooks/")
GATE_LINE = re.compile(r"^\s*(?:run|zwaar)\s+(\S+)\s+(.+?)\s*$", re.M)
ENTRY = re.compile(r"(?:\./)?((?:scripts|crates|benchmarks|\.githooks)/[A-Za-z0-9_./-]+\.(?:py|sh|mjs|js))")
# A repository path named anywhere in a file: a file or a directory.
NAMED_PATH = re.compile(
    r"(?<![A-Za-z0-9_.-])((?:scripts|\.githooks|\.github|benchmarks|corpus|docs|crates|tests)"
    r"/[A-Za-z0-9_./-]*[A-Za-z0-9_-])")
# A bare script name, `remote_gate.sh` or `fixture_env.py`, resolved by name.
BARE_SCRIPT = re.compile(r"(?<![A-Za-z0-9_./-])([A-Za-z0-9_-]+\.(?:py|sh|mjs|js))\b")
PY_IMPORT = re.compile(r"^\s*(?:from\s+([A-Za-z0-9_.]+)\s+import|import\s+([A-Za-z0-9_., ]+))", re.M)
CRATE_INPUTS = ("crates/", "Cargo.toml", "Cargo.lock", ".cargo/", "rust-toolchain.toml")
# More scripts named than this, and a file is a registry of them (see Reach.of).
REGISTRY = 30


def code_of(text: str, suffix: str) -> str:
    """The text without its comments and docstrings, which name scripts to
    explain them rather than to run them: a docstring that says why a guard
    exists made every test that imported it reach what the docstring named."""
    if suffix == ".py":
        text = re.sub(r'(?s)("""|\'\'\').*?\1', "", text)
        return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))
    if suffix == ".sh":
        return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("//"))


def a_clean_environment() -> dict:
    """The caller's environment without GIT_*: the hook exports them (see
    development_gate_scope.py for what inheriting them once cost)."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def changed_paths(root: pathlib.Path) -> set[str] | None:
    """The landing's paths against its base, as development_gate_scope reads them."""
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=root, stderr=subprocess.DEVNULL,
                                       env=a_clean_environment()).decode()
    try:
        base = os.environ.get("LANDING_BASE") or git(
            "merge-base", "HEAD", "refs/remotes/github/master").strip()
        changed = git("diff", "--no-renames", "--name-only", "-z", base).split("\0")
        changed += git("ls-files", "--others", "--exclude-standard", "-z").split("\0")
        return {p for p in changed if p}
    except (subprocess.CalledProcessError, OSError, UnicodeError):
        return None


def test_gates(gate_text: str) -> dict[str, str]:
    """Gate name -> entry file, for the gates that are tests of a script."""
    out: dict[str, str] = {}
    for name, command in GATE_LINE.findall(gate_text):
        m = ENTRY.search(command)
        if not m:
            continue
        entry = m.group(1)
        base = entry.rsplit("/", 1)[-1]
        if (base.startswith("test_") or base.endswith(("_test.py", "_test.sh", ".test.mjs"))
                or "/tests/" in entry):
            # A name used twice keeps the first entry; the gate file runs both,
            # and a second, different entry makes the name unscopable.
            if out.get(name, entry) != entry:
                out[name] = ""
            else:
                out[name] = entry
    return out


class Reach:
    """Every repository path a file reaches, followed file to file."""

    def __init__(self, root: pathlib.Path):
        self.root = root
        self.by_name: dict[str, list[str]] = {}
        for top in ("scripts", ".githooks", "benchmarks"):
            for p in (root / top).rglob("*"):
                if p.is_file() and p.suffix in (".py", ".sh", ".mjs", ".js"):
                    rel = p.relative_to(root).as_posix()
                    self.by_name.setdefault(p.name, []).append(rel)
        self.cache: dict[str, set[str] | None] = {}

    def of(self, entry: str) -> set[str] | None:
        """The inputs of `entry`, or None when some file on the way is unreadable."""
        if entry in self.cache:
            return self.cache[entry]
        seen: set[str] = set()
        todo = [entry]
        while todo:
            rel = todo.pop()
            if rel in seen:
                continue
            seen.add(rel)
            path = self.root / rel
            if path.is_dir() or not path.suffix or path.suffix not in (".py", ".sh", ".mjs", ".js"):
                continue
            # The gate and the hook name every script there is. A change to
            # either runs every test already, so they are an input and not a
            # way through: followed, every test reached everything.
            if rel != entry and any(rel == e or (e.endswith("/") and rel.startswith(e))
                                    for e in EVERYTHING):
                continue
            try:
                text = code_of(path.read_text(encoding="utf-8"), path.suffix)
            except (OSError, UnicodeError):
                self.cache[entry] = None
                return None
            found: list[str] = []
            for named in NAMED_PATH.findall(text):
                named = named.rstrip("./")
                if (self.root / named).exists():
                    found.append(named)
            for bare in BARE_SCRIPT.findall(text):
                found.extend(self.by_name.get(bare, []))
            if path.suffix == ".py":
                for frm, imp in PY_IMPORT.findall(text):
                    for module in ([frm] if frm else [m.strip().split(" ")[0] for m in imp.split(",")]):
                        found.extend(self.by_name.get(module.split(".")[0] + ".py", []))
            if re.search(r"\bcargo\b", text):
                seen.update(CRATE_INPUTS)
            # A REGISTRY IS AN INPUT, NOT A WAY THROUGH. A file that names more
            # scripts than any one test uses -- every_guard_has_a_job.py lists
            # them all -- would make every test reach every script. Its own
            # change still reaches whoever reads it; what it names does not.
            scripts = {f for f in found if f.endswith((".py", ".sh", ".mjs", ".js"))}
            if rel != entry and len(scripts) > REGISTRY:
                continue
            todo.extend(found)
        self.cache[entry] = seen
        return seen


def touches(changed: str, inputs: set[str]) -> bool:
    for i in inputs:
        if changed == i or (i.endswith("/") and changed.startswith(i)) \
                or changed.startswith(i.rstrip("/") + "/"):
            return True
    return False


def out_of_landing(changed: set[str] | None, gate_text: str, root: pathlib.Path) -> dict[str, str]:
    """Test gate -> why the change cannot reach it. Empty means run them all."""
    if changed is None:
        return {}
    if any(c == e or (e.endswith("/") and c.startswith(e)) for c in changed for e in EVERYTHING):
        return {}
    reach = Reach(root)
    out: dict[str, str] = {}
    for name, entry in test_gates(gate_text).items():
        if not entry:
            continue
        inputs = reach.of(entry)
        if inputs is None:
            continue
        if not any(touches(c, inputs) for c in changed):
            out[name] = f"{entry}: none of its {len(inputs)} inputs changed"
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--explain", action="store_true")
    ap.add_argument("--root", type=pathlib.Path, default=ROOT)
    args = ap.parse_args()
    try:
        gate_text = (args.root / GATE).read_text(encoding="utf-8")
    except OSError as e:
        print(f"landing_test_scope: cannot read {GATE}: {e}", file=sys.stderr)
        return 2
    skipped = out_of_landing(changed_paths(args.root), gate_text, args.root)
    if args.explain:
        for name, why in sorted(skipped.items()):
            print(f"{name}\t{why}")
    else:
        print(" ".join(sorted(skipped)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
