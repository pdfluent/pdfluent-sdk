#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every rust-cache step keeps the shared cargo bin. (#636)

WHAT HAPPENED

Release-natives run 38090275381, leg `natives (linux-arm64-musl)` on the
self-hosted runner `wsl-desktop-release` (user root, shared /root/.cargo).
The `Swatinem/rust-cache` post step logged `Cleaning cargo/bin` and removed
`/root/.cargo/bin/rustup`: rust-cache deletes from ~/.cargo/bin what it did
not install itself. Every proxy -- cargo, rustc, all of them -- dangled, and
the next leg failed for want of cargo.

THE RULE

Every `uses: Swatinem/rust-cache@...` step in every workflow carries
`cache-bin: false` in its `with:`. All of them, not only the self-hosted
jobs: runs-on is often an expression, so a per-job judgement would miss
cases, and a cache step never needs to own the bin directory anyway.

WHAT COUNTS AS OFF. The literal text, not what PyYAML makes of it. `false`
in any capitalisation, quoted or not, is off; anything else -- true, `off`,
`no`, `0`, a missing `with:` -- is on. `off` reads as boolean false under
YAML 1.1 while the runner reads YAML 1.2, where it is the string "off" and
therefore truthy; judging the parsed value would approve a step that still
cleans.

`--self-test` runs the inline fixtures: a missing `with:`, a `with:`
without cache-bin, `cache-bin: true`, and a correct step -- red, red, red,
green -- plus the quoted-`false` and `off` spellings the literal rule
exists for.

Exit codes:
  0  every rust-cache step keeps the bin
  1  one of them does not
  2  nothing could be read: no workflow directory, or so few workflows or
     cache steps found that the scan is not looking at what it thinks it is
"""
from __future__ import annotations

import argparse
import pathlib
import sys

try:
    import yaml
except ImportError:  # pragma: no cover - the lane installs pyyaml
    print("every_rust_cache_keeps_cargo_bin: pyyaml is not installed, "
          "so no workflow could be read.", file=sys.stderr)
    sys.exit(2)

ROOT = pathlib.Path(__file__).resolve().parents[2]

CACHE_ACTION = "Swatinem/rust-cache"


def _cache_steps(node, out: list) -> None:
    """Every rust-cache `uses:` scalar with its step's `with:` node.

    Walks the composed document rather than the loaded one: only the nodes
    still know the line they were read from, and only the scalars still
    carry the literal text -- `off` is boolean false once parsed.
    """
    if isinstance(node, yaml.MappingNode):
        uses = None
        with_ = None
        for key, value in node.value:
            if isinstance(key, yaml.ScalarNode) and key.value == "uses":
                uses = value
            elif isinstance(key, yaml.ScalarNode) and key.value == "with":
                with_ = value
        if (isinstance(uses, yaml.ScalarNode)
                and uses.value.startswith(CACHE_ACTION)):
            out.append((uses, with_))
        for key, value in node.value:
            _cache_steps(key, out)
            _cache_steps(value, out)
    elif isinstance(node, yaml.SequenceNode):
        for child in node.value:
            _cache_steps(child, out)


def _keeps_the_bin(with_) -> bool:
    """Does this step's `with:` switch the bin cleanup off?

    Judged on the literal scalar: `false` in any capitalisation, quoted or
    not. "String or bool false both OK" is about how the author wrote it --
    both spellings of `false` reduce to the same four letters, while `off`
    reduces to boolean false under YAML 1.1 and must not pass.
    """
    if not isinstance(with_, yaml.MappingNode):
        return False
    for key, value in with_.value:
        if isinstance(key, yaml.ScalarNode) and key.value == "cache-bin":
            return (isinstance(value, yaml.ScalarNode)
                    and value.value.strip().lower() == "false")
    return False


def check_text(name: str, text: str) -> tuple[list[str], int]:
    """The findings and the rust-cache step count for one workflow's text."""
    try:
        document = yaml.compose(text)
    except yaml.YAMLError as bad:
        return [f"{name}: could not be parsed: {bad}"], 0
    found: list = []
    if document is not None:
        _cache_steps(document, found)
    findings = []
    for uses, with_ in found:
        if not _keeps_the_bin(with_):
            line = uses.start_mark.line + 1
            findings.append(
                f"{name}:{line}: its rust-cache step cleans ~/.cargo/bin "
                f"-- add `cache-bin: false` to its `with:`.")
    return findings, len(found)


def check_workflows(workflows: pathlib.Path) -> tuple[list[str], int, int]:
    """The findings, file count and step count over a workflow directory."""
    findings: list[str] = []
    files = 0
    steps = 0
    for pattern in ("*.yml", "*.yaml"):
        for path in sorted(workflows.glob(pattern)):
            files += 1
            own, count = check_text(path.name,
                                    path.read_text(errors="replace"))
            findings.extend(own)
            steps += count
    return findings, files, steps


# The inline fixtures. Every one puts `- uses:` on line 7, so the self-test
# holds each red to naming `fixture.yml:7:` and not just the file.
_MISSING_WITH = """\
name: fixture
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: Swatinem/rust-cache@v2
"""

_WITH_WITHOUT_CACHE_BIN = """\
name: fixture
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: Swatinem/rust-cache@v2
        with:
          key: fixture-${{ matrix.target }}
"""

_CACHE_BIN_TRUE = """\
name: fixture
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: Swatinem/rust-cache@v2
        with:
          cache-bin: true
"""

_CORRECT = """\
name: fixture
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: Swatinem/rust-cache@v2
        with:
          cache-bin: false
"""

_QUOTED_FALSE = """\
name: fixture
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: Swatinem/rust-cache@v2
        with:
          key: fixture-${{ matrix.target }}
          cache-bin: "false"
"""

_OFF_IS_NOT_FALSE = """\
name: fixture
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: Swatinem/rust-cache@v2
        with:
          cache-bin: off
"""


def self_test() -> int:
    """The guard against its own fixtures: red, red, red, green."""
    cases = [
        ("a step without `with:`", _MISSING_WITH, True),
        ("a `with:` without cache-bin", _WITH_WITHOUT_CACHE_BIN, True),
        ("`cache-bin: true`", _CACHE_BIN_TRUE, True),
        ("a correct step", _CORRECT, False),
        ("a quoted 'false' beside a key", _QUOTED_FALSE, False),
        ("`cache-bin: off`", _OFF_IS_NOT_FALSE, True),
    ]
    failures = 0
    for name, text, red in cases:
        findings, steps = check_text("fixture.yml", text)
        if steps != 1:
            print(f"[self-test] FAIL: {name}: found {steps} cache "
                  f"step(s), not 1")
            failures += 1
            continue
        if bool(findings) != red:
            print(f"[self-test] FAIL: {name}: expected "
                  f"{'red' if red else 'green'}, got "
                  f"{'red' if findings else 'green'}")
            failures += 1
            continue
        if red and not findings[0].startswith("fixture.yml:7:"):
            print(f"[self-test] FAIL: {name}: finding names no "
                  f"file:line: {findings[0]!r}")
            failures += 1
            continue
        print(f"[self-test] ok: {name} -> {'red' if red else 'green'}")
    if failures:
        print(f"[self-test] {failures} of {len(cases)} fixture(s) failed")
        return 1
    print(f"[self-test] OK: all {len(cases)} fixtures behave")
    return 0


# A scan that finds nothing approves everything.
MIN_WORKFLOWS = 20
MIN_CACHE_USES = 10


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(ROOT),
                        help="the checkout to read (a fixture tree, in the test)")
    parser.add_argument("--self-test", action="store_true",
                        help="run the inline fixtures instead of reading the tree")
    arguments = parser.parse_args()
    if arguments.self_test:
        return self_test()
    workflows = pathlib.Path(arguments.repo).resolve() / ".github" / "workflows"
    if not workflows.is_dir():
        print(f"every_rust_cache_keeps_cargo_bin: no {workflows}; "
              f"nothing was checked.")
        return 2

    findings, files, steps = check_workflows(workflows)

    if files < MIN_WORKFLOWS or steps < MIN_CACHE_USES:
        print(f"every_rust_cache_keeps_cargo_bin: only {files} workflow(s) and "
              f"{steps} rust-cache step(s) found, and there should be at "
              f"least {MIN_WORKFLOWS} and {MIN_CACHE_USES}.")
        print("  The scan is not reading the workflows it thinks it is. A scan that finds "
              "nothing approves everything.")
        return 2

    if findings:
        print(f"every_rust_cache_keeps_cargo_bin: {len(findings)} problem(s) "
              f"over {files} workflow(s).")
        for finding in findings:
            print(f"  {finding}")
        print("")
        print("A rust-cache step never cleans the shared cargo bin:")
        print("  - uses: Swatinem/rust-cache@v2")
        print("    with:")
        print("      cache-bin: false")
        print("Without it the post step deletes from ~/.cargo/bin what it did not")
        print("install itself, and on a shared self-hosted runner that is rustup (#636).")
        return 1

    print(f"every_rust_cache_keeps_cargo_bin: OK -- {files} workflow(s), {steps} "
          f"rust-cache step(s), every one keeps the bin.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
