#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The WASM bundle does not carry the substitute fonts, and stays under B07.

WHY THIS IS A GUARD AND NOT A CARGO FEATURE (#383)

pdf-substitute-fonts carries twelve Liberation faces, 4.2 MB. Claim B07 is
published and VERIFIED at 2.5 MB brotli / 9.6 MB unpacked, so those bytes cannot
be in the browser bundle.

The obvious mechanism -- a cargo feature, off for wasm -- does not hold it.
xfa-wasm reaches pdf-manip along four paths (directly, via pdfluent, via
pdf-redact, via pdf-text-format) and cargo unifies features across the whole
graph, so a single dependant with defaults on switches it back on for all of
them. That was measured on 08-09-2026, and it is why pdf-manip declares
pdf-substitute-fonts under `[target.'cfg(not(target_arch = "wasm32"))']` instead.

This checks the invariant that replaced the feature, in three ways that fail
independently.

The manifests are the first and the only one that always runs: every declaration
of pdf-substitute-fonts across the workspace must sit under a
`cfg(not(target_arch = "wasm32"))` target table. It reads text, needs no cargo
and no network, so it answers the same on a hosted pull-request runner as it does
here -- which is the point, since the edge that breaks this is one line in a
Cargo.toml that a stranger can write.

`cargo tree --target wasm32-unknown-unknown` is the second, and it is the one
that would catch a path the text scan cannot see. It runs when cargo is there
and announces a skip when it is not, because a guard that quietly needs a
toolchain is a guard that quietly does not run.

The size of a built bundle is the third, and runs only when one is lying around.
All three are needed: a manifest can be right while some other change doubles the
bundle, and a bundle can be under budget today while the fonts sit in it and the
next face pushes it over.
"""
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(os.environ.get("PDFLUENT_BUNDLE_GUARD_ROOT")
                    or pathlib.Path(__file__).resolve().parents[2])

# The crate whose bytes must not reach a browser, and the target that must not
# see it.
FONT_CRATE = "pdf-substitute-fonts"
WASM_TARGET = "wasm32-unknown-unknown"
WASM_PACKAGE = "xfa-wasm"

# B07 publishes the unpacked size of the bundle. The ceiling is that number
# rounded up to the next whole megabyte, not a number chosen to fit what we
# happen to build today: a bundle that grows past what the site says is a broken
# claim whether or not it still loads.
#
# THIS NUMBER WAS WRONG FOR A MONTH, AND THE RULE ABOVE IS WHY IT MATTERED.
#
# It read 9.6 MB with a 10 MiB ceiling until 10-09-2026. The published package
# had been over it since August: `@pdfluent/sdk-wasm@1.0.0-beta.17.5`, read out
# of its own npm tarball on 09-09-2026, carries a 13,808,104-byte `.wasm`, and
# the 1.0.0 dry run measured 14,619,146. Nothing caught it, because this half of
# the guard only weighs a bundle that exists on the machine and no lane builds
# one where the gate can see it.
#
# The claim was corrected before this line was: B07 was re-measured on the 1.0.0
# dry run and marked DATED. That order is the whole of it -- raising a ceiling to
# meet a bundle is moving the goalposts, and raising it to follow a corrected
# published claim is not. If this number ever moves again without CLAIMS.md
# moving first, it is the wrong repair. (#519)
#
# Megabytes here are 10^6, the unit the claim is written in. The old line said
# "MB" and compared against MiB, which is a fifth of the slack it looked like.
B07_UNPACKED_MB = 14.6
CEILING_BYTES = 15_000_000

# Where a built bundle lands, in the order wasm-pack and the workflows put it.
BUNDLE_NAMES = ("xfa_wasm_bg.wasm", "pdfluent_bg.wasm")
BUNDLE_DIRS = ("pkg", "crates/xfa-wasm/pkg", "bindings/wasm/pkg", "dist")


def cargo() -> str:
    """Where cargo actually is; a hosted runner may not put it on PATH."""
    from shutil import which
    found = which("cargo")
    if found:
        return found
    for candidate in (
        pathlib.Path(os.environ.get("CARGO_HOME", "")) / "bin" / "cargo",
        pathlib.Path.home() / ".cargo" / "bin" / "cargo",
    ):
        if candidate.is_file():
            return str(candidate)
    return "cargo"


def every_declaration_is_target_conditional() -> list[str]:
    """No manifest may name the font crate outside a not-wasm32 target table.

    Text, not cargo: this is what makes the guard runnable by a stranger's pull
    request, and a dependency edge is added by writing a line in a manifest, so
    the manifest is where the rule belongs.

    A `[target.'cfg(...)']` table governs every line until the next `[table]`
    header, which is all the parsing this needs -- and doing it by hand rather
    than with a TOML library keeps the guard's own dependencies at zero.
    """
    problems = []
    for manifest in sorted(REPO.glob("crates/*/Cargo.toml")):
        table = ""
        for number, row in enumerate(
            manifest.read_text(encoding="utf-8", errors="ignore").split("\n"), 1
        ):
            stripped = row.strip()
            if stripped.startswith("[") and not stripped.startswith("[["):
                table = stripped
                continue
            if stripped.startswith("#") or "=" not in stripped:
                continue
            name = stripped.split("=", 1)[0].strip().strip('"')
            if name != FONT_CRATE:
                continue
            if 'target_arch = "wasm32"' in table and table.startswith(
                "[target.'cfg(not("
            ):
                continue
            problems.append(
                f"{manifest.relative_to(REPO)}:{number} declares {FONT_CRATE} "
                f"under {table or '[dependencies]'}, which wasm32 also reads, so "
                f"4.2 MB of fonts reach the browser bundle. It belongs under "
                f"[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]"
            )
    return problems


def the_font_crate_is_out_of_the_wasm_graph() -> list[str]:
    """`cargo tree` for wasm32 must not mention the font crate.

    `-i` inverts: it prints who depends on the crate. An empty answer is the
    pass. A missing wasm32 target is NOT a pass and says so -- rustup on this
    machine either has the target or the check cannot speak.
    """
    try:
        done = subprocess.run(
            [cargo(), "tree", "--target", WASM_TARGET,
             "-p", WASM_PACKAGE, "-i", FONT_CRATE],
            cwd=REPO, capture_output=True, text=True, timeout=300,
        )
    except FileNotFoundError:
        return ["SKIPPED (not a pass): cargo is not on PATH"]
    except subprocess.TimeoutExpired:
        return ["cargo tree did not finish within 300s"]

    both = (done.stdout or "") + (done.stderr or "")
    if "nothing to print" in both:
        return []
    if f"{FONT_CRATE} v" in done.stdout:
        who = [r.strip() for r in done.stdout.strip().split("\n")][:12]
        return [f"{FONT_CRATE} is in the {WASM_TARGET} graph of {WASM_PACKAGE}, "
                f"so 4.2 MB of fonts ship to browsers and claim B07 "
                f"({B07_UNPACKED_MB} MB unpacked) is no longer true. Path:",
                *[f"    {r}" for r in who]]
    if done.returncode != 0:
        return [f"cargo tree failed ({done.returncode}): "
                f"{(done.stderr or '').strip().splitlines()[-1:] or ['no output']}"]
    # Exit 0, no crate, no "nothing to print": the shape changed under us and a
    # silent pass here is exactly the failure this file exists to prevent.
    return [f"cargo tree gave an answer this guard does not recognise, so it "
            f"cannot say whether {FONT_CRATE} is in the bundle: "
            f"{done.stdout.strip()[:200]!r}"]


def a_built_bundle_is_under_the_ceiling() -> list[str]:
    """Any .wasm we can find must be under B07's published size.

    Absent a build there is nothing to weigh; that is stated rather than passed
    over, because "no bundle found" and "bundle is fine" must not read the same.
    """
    found = []
    for directory in BUNDLE_DIRS:
        for name in BUNDLE_NAMES:
            path = REPO / directory / name
            if path.is_file():
                found.append(path)
    if not found:
        print("[bundle] no built .wasm on this machine; the manifest and graph "
              "halves above are what hold the size here")
        return []
    out = []
    for path in found:
        size = path.stat().st_size
        mb = size / 1_000_000
        if size > CEILING_BYTES:
            out.append(f"{path.relative_to(REPO)} is {mb:.1f} MB unpacked "
                       f"({size} bytes), over the "
                       f"{CEILING_BYTES / 1_000_000:.0f} MB ceiling that claim "
                       f"B07 ({B07_UNPACKED_MB} MB) sets. If the bundle is meant "
                       f"to be this size, B07 is corrected first and this "
                       f"ceiling follows it -- never the other way round (#519)")
        else:
            print(f"[bundle] {path.relative_to(REPO)}: {mb:.1f} MB unpacked, "
                  f"under the {CEILING_BYTES / 1_000_000:.0f} MB ceiling")
    return out


def main() -> int:
    problems = every_declaration_is_target_conditional()
    graph = the_font_crate_is_out_of_the_wasm_graph()
    skipped = [p for p in graph if p.startswith("SKIPPED")]
    if skipped:
        # The manifest half above still ran and still counts; only the cargo
        # half is unavailable, and saying which one is missing is the difference
        # between a guard that skipped and a guard that passed.
        print(skipped[0])
    else:
        problems += graph
    problems += a_built_bundle_is_under_the_ceiling()
    if problems:
        print("[bundle] FAIL:")
        for p in problems:
            print(f"  {p}")
        return 1
    print(f"[bundle] OK: every declaration of {FONT_CRATE} is target-conditional, "
          f"it is not in the {WASM_TARGET} graph of {WASM_PACKAGE}, and no built "
          f"bundle is over {CEILING_BYTES / 1_000_000:.0f} MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
