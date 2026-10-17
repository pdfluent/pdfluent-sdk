#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The wasm-bundle std guard, in every lane, in a second (#623).

`crates/xfa-wasm/tests/wasm_unsafe_std_guard.rs` refuses `Instant::now()`,
`SystemTime::now()` and `thread::spawn()` in the crates that ship inside the
WASM bundle: they compile for wasm32 and panic there. It is a cargo test of
xfa-wasm, so it ran only where xfa-wasm was built -- and a landing that touched
pdf-engine alone did not build it. On 27-09-2026 `structured_batch.rs` landed an
`Instant::now()` that way (f6f5f1ffc) and the guard stood red on master until
T4 found it.

The rule is a source scan and needs no compiler, so this runs it in the fast
lane. THE RULE STAYS IN ONE PLACE: the crate list, the forbidden calls and the
allowlist are read out of the Rust test itself, so the two cannot drift -- a
list this file kept of its own would be a second rule nobody updates.

Exit 0: clean. 1: a forbidden call outside the allowlist. 2: the Rust test's
lists could not be read (a guard that reads nothing must not pass).
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
RUST_GUARD = REPO / "crates" / "xfa-wasm" / "tests" / "wasm_unsafe_std_guard.rs"
CLOCKS = ("pdf-manip/src/clock.rs", "pdf-sign/src/clock.rs")


def const_block(source: str, name: str) -> str:
    match = re.search(rf"const {name}: [^=]+= &\[(.*?)\];", source, re.S)
    if not match:
        raise ValueError(f"no `const {name}` in {RUST_GUARD.name}")
    return match.group(1)


def strings(block: str) -> list[str]:
    """String literals of a Rust list, comment lines left out."""
    kept = "\n".join(l for l in block.splitlines() if not l.strip().startswith("//"))
    return re.findall(r'"((?:[^"\\]|\\.)*)"', kept, re.S)


def rules(source: str) -> tuple[list[str], list[str], list[str]]:
    crates = strings(const_block(source, "WASM_CRATES"))
    forbidden = strings(const_block(source, "FORBIDDEN"))
    # ALLOWED is (path, reason) pairs: the path is the first literal of each
    # pair, and every path names a `.rs` file.
    allowed = [s for s in strings(const_block(source, "ALLOWED")) if s.endswith(".rs")]
    if not crates or not forbidden:
        raise ValueError("the crate list or the forbidden calls came out empty")
    return crates, forbidden, allowed


def violations(crates_dir: pathlib.Path, crates: list[str], forbidden: list[str],
               allowed: list[str]) -> list[str]:
    found = []
    for krate in crates:
        src = crates_dir / krate / "src"
        if not src.is_dir():
            continue
        for path in sorted(src.rglob("*.rs")):
            rel = path.relative_to(crates_dir).as_posix()
            if rel in allowed or rel.endswith(CLOCKS):
                continue
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            # The same scope as the Rust test: up to the first top-level
            # `#[cfg(test)]`, comment lines skipped.
            end = next((i for i, l in enumerate(lines) if l.startswith("#[cfg(test)]")),
                       len(lines))
            for number, line in enumerate(lines[:end], 1):
                if line.lstrip().startswith("//"):
                    continue
                for needle in forbidden:
                    if needle in line:
                        found.append(f"{rel}:{number} -- {needle}")
    return found


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--crates-dir", type=pathlib.Path, default=REPO / "crates")
    ap.add_argument("--guard", type=pathlib.Path, default=RUST_GUARD)
    args = ap.parse_args()
    try:
        crates, forbidden, allowed = rules(args.guard.read_text())
    except (OSError, ValueError) as exc:
        print(f"wasm-std: SKIPPED (not a pass): {exc}", file=sys.stderr)
        return 2
    found = violations(args.crates_dir, crates, forbidden, allowed)
    if found:
        print("wasm-std: FAIL: these compile on wasm32 and panic there "
              "(RuntimeError: unreachable in JavaScript):", file=sys.stderr)
        for line in found:
            print(f"  {line}", file=sys.stderr)
        print(f"Use pdf_manip::clock::unix_now_secs(), or cfg-gate the code on "
              f"not(target_arch = \"wasm32\") and add it to ALLOWED in "
              f"{args.guard.relative_to(REPO) if args.guard.is_relative_to(REPO) else args.guard} "
              "with the reason.", file=sys.stderr)
        return 1
    print(f"wasm-std: OK: {len(crates)} crate(s), {len(forbidden)} forbidden call(s), "
          f"{len(allowed)} allowed site(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
