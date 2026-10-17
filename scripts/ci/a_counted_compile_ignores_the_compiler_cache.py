#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A gate that COUNTS compiler errors may not run under a compiler cache.

On 09-09-2026 one commit measured 87 non-compiling site programs on the
operator's Mac and 97 on a bare Linux instance -- same rustc 1.94.0, same
lockfile, same blocks. The Mac's `~/.cargo/config.toml` sets sccache as
`RUSTC_WRAPPER`; sccache invokes rustc with `-o`, and that run stops after name
resolution, so every `E0599` and every `E0277` in the same crate was never
emitted. Ten site examples calling methods that exist nowhere in this workspace
were counted as compiling, and had been for as long as the baseline was taken on
a machine with a cache (#474, #476).

WHY THE RULE IS ABOUT COUNTING AND NOT ABOUT COMPILING.

Most gates here compile in order to reach a verdict: the build either succeeds
or it does not, and a cache that changed that would show up immediately as a red
build. Clearing the wrapper for those would throw away the one thing that makes
a landing lane on this machine affordable -- measured in the cargo config itself:
813 dependencies, 54 worktrees, one build each without it.

The gates this is about are the ones that turn diagnostics into a NUMBER and
write that number down. There a suppressed error is not a red, it is a smaller
number, and a smaller number reads as progress. That is the whole failure: the
ratchet was green, on both machines, for a year of examples nobody could copy.

WHAT IS DERIVED AND WHAT IS WRITTEN DOWN.

The set is derived from the tree: a script under `scripts/ci/` that runs cargo or
rustc AND parses rustc's error codes out of the output is a counting gate. What
is written down is the exemption, with its reason -- because a script that reads
diagnostics for something other than a count is a judgement call, and a judgement
call belongs in a sentence somebody signed rather than in a regex.

# NO-FLOOR: it reads a directory it also counts. A search that suddenly finds no
# counting gates at all is reported as a broken search, not as a clean tree.
"""
from __future__ import annotations

import pathlib
import re
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
MAP = WORTEL / "scripts" / "ci"

# Invoking the compiler: the name as an argv element, which is how every caller
# here spells it. A shell script that runs `cargo build` is not in scope -- see
# the docstring on why a verdict is not a count.
ROEPT_COMPILER = re.compile(r"""["']\s*(cargo|rustc)\s*["']""")
# Reading rustc's own error vocabulary back out of the output. A gate that only
# looks at the exit code does not match, which is the line this rule draws.
LEEST_DIAGNOSTIEK = re.compile(r"error\\?\[|E0\d{3}|--message-format")
# Both spellings, because cargo accepts either and clearing one leaves the other
# in force -- which is a fix that looks like a fix and changes nothing.
WIST_CACHE = (re.compile(r"""RUSTC_WRAPPER"""),
              re.compile(r"""CARGO_BUILD_RUSTC_WRAPPER"""))

# A counting gate that may keep the cache, and why. Empty is the resting state.
UITGEZONDERD: dict[str, str] = {
    # Its subject IS the wrapper: it plants one and asserts the gate ignores it,
    # so it has to be able to name it without clearing it.
    "test_site_blocks_register.py":
        "the test that holds the rule; it sets a wrapper on purpose and asserts "
        "the real build() drops it",
    "test_a_counted_compile_ignores_the_compiler_cache.py":
        "its test, which builds fixtures that mention both variables",
}


def main() -> int:
    scripts = sorted(p for p in MAP.rglob("*.py") if p.is_file())
    if len(scripts) < 25:
        print(f"FLOOR: {len(scripts)} python file(s) under {MAP.relative_to(WORTEL)}, "
              "expected >= 25. The search is broken -- this is not a green.",
              file=sys.stderr)
        return 1

    tellend, problemen = [], []
    for pad in scripts:
        tekst = pad.read_text(encoding="utf-8", errors="replace")
        if not (ROEPT_COMPILER.search(tekst) and LEEST_DIAGNOSTIEK.search(tekst)):
            continue
        tellend.append(pad.name)
        if pad.name in UITGEZONDERD:
            continue
        ontbreekt = [p.pattern for p in WIST_CACHE if not p.search(tekst)]
        if ontbreekt:
            problemen.append(
                f"{pad.relative_to(WORTEL)} counts what the compiler says and never "
                f"mentions {', '.join(ontbreekt)}. A cache that suppresses a class of "
                f"diagnostic makes this gate report a smaller number, and a smaller "
                f"number reads as progress. Clear both in the environment you hand "
                f"cargo, or add an entry here saying why this one may keep it."
            )

    for naam, reden in UITGEZONDERD.items():
        if naam not in tellend:
            problemen.append(
                f"{naam} is written down as an exempt counting gate and is no longer "
                f"one. Drop the entry: an exemption that outlives its subject is how "
                f"the next real one goes unnoticed."
            )

    print(f"[counted-compile] {len(tellend)} gate(s) count what the compiler says; "
          f"{len(UITGEZONDERD)} exempt with a reason")
    if problemen:
        for p in problemen:
            print(f"  {p}", file=sys.stderr)
        return 1
    print("[counted-compile] OK: none of them lets a compiler cache change the number.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
