#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The lint lane must read test code, because the machine that bills us does.

`workspace / test` on the first wave run on a bought machine (run 34088643890,
07-09-2026) could not compile `formcalc-interpreter` or `pdfluent-jbig2` as lib
tests. Neither had regressed that morning: a `///` on a `thread_local!` and
three ignored `#[must_use]` results had been sitting in `#[cfg(test)]` code for
weeks, and every local lane called them green (#361).

Both are warnings. The wave sets `RUSTFLAGS: -D warnings`, so there they are
errors -- and `scripts/ci/run_clippy.sh`, the one local lane that also denies
warnings, passed no `--all-targets` and therefore compiled library code only.
Test modules and `tests/` files were linted by nothing at all before the branch
left the machine. The suites then queued for hours on the desktop, so the colour
that would have said so was never read either.

WHAT THIS COVERS

The flags, and then the effect of the flags, because "the string --all-targets
appears in a shell script" is not the property that matters:

  * `run_clippy.sh` denies warnings and asks for every target;
  * a crate carrying exactly the two defect shapes, with both inside
    `#[cfg(test)]`, is REFUSED by the flag list the script actually ships;
  * the same crate passes when `--all-targets` is dropped from that list -- so
    the test cannot pass by clippy happening to be strict about something else,
    and dropping the flag from the script turns this red twice over.

The flag list is read out of the script rather than restated here. A copy of it
would be a second answer to the question the script exists to answer.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
LANE = ROOT / "scripts/ci/run_clippy.sh"

# The two shapes that were red on the wave, side by side, both under
# `#[cfg(test)]` so that a lane without `--all-targets` never sees them:
# a doc comment on a macro invocation (which documents nothing, and is
# `unused_doc_comments`), and a `#[must_use]` result thrown away
# (`unused_must_use`).
PROBE_LIB = """\
// Not `Result<_, ()>`: clippy refuses that shape in library code, and the
// probe has to be clean everywhere except the test module.
pub fn answers() -> Result<(), std::fmt::Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    /// A macro invocation is not an item, so this documents nothing.
    thread_local! {
        static SEEN: std::cell::Cell<usize> = std::cell::Cell::new(0);
    }

    #[test]
    fn probe() {
        SEEN.with(|c| c.set(1));
        super::answers();
    }
}
"""

PROBE_MANIFEST = """\
[package]
name = "clippy-lane-probe"
version = "0.0.0"
edition = "2021"
publish = false

# Its own workspace root: without this the probe would be adopted by whatever
# workspace happens to be above the temporary directory and inherit its lints.
[workspace]
"""

failures: list[str] = []


def fail(line: str) -> None:
    failures.append(line)


def shipped_flags() -> list[str]:
    """The flags run_clippy.sh puts after the package list, as it ships them."""
    text = LANE.read_text(encoding="utf-8")
    match = re.search(r'cargo clippy "\$\{pkgs\[@\]\}"(.*)$', text, re.M)
    if not match:
        fail(f"{LANE.name}: no `cargo clippy \"${{pkgs[@]}}\"` invocation found")
        return []
    return match.group(1).split()


def clippy(crate: pathlib.Path, flags: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["cargo", "clippy", "--offline", *flags],
        cwd=crate,
        capture_output=True,
        text=True,
        check=False,
    )


def lints_named(output: str) -> set[str]:
    """Lint names in clippy's output, with `-` and `_` spelled the same way."""
    return {name.replace("-", "_") for name in re.findall(r"[a-z]+(?:[-_][a-z]+)+", output)}


def main() -> int:
    flags = shipped_flags()
    if "--all-targets" not in flags:
        fail(
            "scripts/ci/run_clippy.sh does not pass --all-targets, so no local lane "
            "lints #[cfg(test)] code; the wave does, under -D warnings (#361)"
        )
    if "-D" not in flags or "warnings" not in flags:
        fail("scripts/ci/run_clippy.sh no longer denies warnings")
    if not flags:
        return report()

    if shutil.which("cargo") is None:
        print("[clippy-lane] FATAL: cargo is not on PATH", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="clippy-lane-") as tmp:
        crate = pathlib.Path(tmp) / "probe"
        (crate / "src").mkdir(parents=True)
        (crate / "Cargo.toml").write_text(PROBE_MANIFEST, encoding="utf-8")
        (crate / "src" / "lib.rs").write_text(PROBE_LIB, encoding="utf-8")

        refused = clippy(crate, flags)
        if refused.returncode == 0:
            fail(
                "the shipped flag list accepts a crate whose test module has both "
                "wave defects in it: " + " ".join(flags)
            )
        else:
            seen = lints_named(refused.stderr)
            for lint in ("unused_doc_comments", "unused_must_use"):
                if lint not in seen:
                    fail(
                        f"the shipped flag list refused the probe, but not for {lint}; "
                        "the refusal has to be the defect and not something beside it"
                    )

        # The control. Without --all-targets the very same crate is clean, which
        # is what makes the run above evidence about the flag rather than about
        # clippy's mood.
        library_only = [f for f in flags if f != "--all-targets"]
        accepted = clippy(crate, library_only)
        if accepted.returncode != 0:
            fail(
                "the probe is refused even without --all-targets, so it proves "
                "nothing about the flag:\n" + accepted.stderr[-2000:]
            )

    return report()


def report() -> int:
    if failures:
        print("[clippy-lane] FAIL", file=sys.stderr)
        for line in failures:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print("[clippy-lane] OK: the local lint lane denies warnings in test code too")
    return 0


if __name__ == "__main__":
    sys.exit(main())
