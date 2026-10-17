#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The fast-lane wasm std guard refuses what the Rust test refuses (#623)."""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "wasm_bundle_calls_no_panicking_std.py"
RUST = (REPO / "crates" / "xfa-wasm" / "tests" / "wasm_unsafe_std_guard.rs").read_text()


def run(crates: pathlib.Path, guard: pathlib.Path) -> int:
    return subprocess.run([sys.executable, str(GUARD), "--crates-dir", str(crates),
                           "--guard", str(guard)], capture_output=True).returncode


def main() -> int:
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        crates = root / "crates"
        guard = root / "guard.rs"
        guard.write_text(RUST)
        src = crates / "pdf-engine" / "src"
        src.mkdir(parents=True)
        (src / "lib.rs").write_text("pub fn f() {}\n")
        if run(crates, guard) != 0:
            failures.append("a clean crate was refused")
        (src / "timer.rs").write_text("pub fn t() { let _ = std::time::Instant::now(); }\n")
        if run(crates, guard) != 1:
            failures.append("an Instant::now() in a wasm crate passed (the landing of f6f5f1ffc)")
        (src / "timer.rs").write_text("// Instant::now( is described here\n"
                                      "#[cfg(test)]\nmod t { fn x() { std::time::Instant::now(); } }\n")
        if run(crates, guard) != 0:
            failures.append("a comment or a test module was refused")
        (src / "batch.rs").write_text("pub fn b() { std::thread::spawn(|| ()); }\n")
        if run(crates, guard) != 0:
            failures.append("an allowlisted file of the Rust test was refused")
        (crates / "other" / "src").mkdir(parents=True)
        (crates / "other" / "src" / "lib.rs").write_text("fn x() { std::time::Instant::now(); }\n")
        if run(crates, guard) != 0:
            failures.append("a crate outside the Rust test's list was scanned")
        guard.write_text("// no lists here\n")
        if run(crates, guard) != 2:
            failures.append("a guard file without its lists passed instead of refusing to judge")
    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print("test-wasm-std: OK: 6 cases, the rule read from the Rust test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
