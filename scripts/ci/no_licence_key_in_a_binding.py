#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""No binding reads a licence key, and no code path refuses work for want of one.

WHY THIS IS A GATE AND NOT A CONVENTION

There is no licence key. That is the product decision of 25 August 2026 (#199),
carried out in #226: the source is published, so any technical check can be
removed in minutes by whoever holds the code, and a check would therefore only
ever inconvenience the people who intended to pay. `LICENSE-COMMERCIAL` §5 now
states as fact that nothing in the software reads a key, refuses a capability or
marks output -- and a licence text that describes the binary is a promise the
binary has to keep.

What makes this a gate rather than a note is how the checks got there the first
time. Not one of them was a decision: `require_capability` arrived as a helper
and spread to sixty-five call sites, six bindings each grew an `activate` entry
point because the one next to it had one, and a free-tier watermark sat inside
`embed_fonts` for months where nobody reading the PDF/A code would look for it.
Every one of those is a small, locally-sensible edit. The class only becomes
visible when something looks at all six surfaces at once, which is this file.

WHAT IT REFUSES

  the key surface      an entry point that takes a licence key: `set_license_key`,
                       `activate_license`, `pdfluent_license_activate_*`, and the
                       binding spellings of the same thing.
  the environment      any read of `PDFLUENT_LICENSE_KEY` or `PDFLUENT_LICENSE_FILE`.
                       This was the quiet one -- an env read needs no API and no
                       documentation, so it survives a review of the public
                       surface untouched.
  the gate             `require_capability`, a `Capability` / `Tier` type, or a
                       status code that says a caller is not licensed for
                       something.
  the mark             output stamped because no key was present -- the
                       "Free Tier" watermark and the "PDFluent trial" notice.

WHAT IT DOES NOT REFUSE, DELIBERATELY

The words "licence" and "license" themselves, which appear in every file header
in this repository and in the name of every LICENSE file. This guard is about a
key being *read*, not about the licence being *mentioned*. It is also silent on
`docs/` and on the website: prose that describes a key is wrong for a different
reason and is fixed by reading it, while code that reads one is invisible until
someone runs the binary without a key -- which is nobody here, because the
machines that run the suite have none.

The exemption list is for text that must name the removed surface in order to
say it is gone: the licence text itself, the changelog, and this file.

Exit codes:
    0  no scanned file reads a key or withholds a capability
    1  one does, or the scan was too small to have looked
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

# The surfaces a key could come back through. Only code -- prose lives in docs/
# and is a different failure with a different fix.
SCOPE = (
    "crates/",
    "bindings/",
    "tools/",
)

EXTENSIES = (".rs", ".py", ".pyi", ".js", ".cjs", ".mjs", ".ts", ".d.ts", ".java", ".cs", ".c", ".h")

# TESTS ARE OUT OF SCOPE, and this is the one exclusion that decides whether the
# guard is usable. The test that proves the key is gone has to name the thing
# that is gone: it sets PDFLUENT_LICENSE_KEY and asserts the output does not
# change, it asserts `setLicenseKey` is not on the module, it asserts no page
# carries the "PDFluent trial" notice. A guard that refused those would refuse
# exactly the evidence it exists to protect, and the first person to hit that
# would delete the tests rather than the guard.
#
# The split is therefore: this guard watches what ships, the tests watch what it
# does. Neither covers the other, and both are named in the failure text so a
# reader is not left guessing which half caught them.
TESTPADEN = re.compile(r"(?:^|/)(?:tests?|__tests__|examples)/|\.test\.[cm]?[jt]s$|(?:^|/)test_[^/]+\.py$")

# An inline Rust test module is a test in a source file. What a customer
# compiles into their product is what stands before it -- `#[cfg(test)]` is
# stripped from a release build by definition -- so the scan stops there rather
# than skipping the whole file, which would blind it to the module above.
# `#[cfg(all(test, not(target_arch = "wasm32")))]` is the spelling xfa-wasm uses,
# so matching only the bare `#[cfg(test)]` would have missed that whole module --
# it did, on the first run of this guard.
CFG_TEST = re.compile(r"^\s*#\[cfg\((?:test\)|all\(test[,)])", re.M)

# Files allowed to name the removed surface, because their subject IS that it was
# removed. Kept short on purpose: every entry is a place this guard cannot see.
VRIJGESTELD = {
    "scripts/ci/no_licence_key_in_a_binding.py",
    "scripts/ci/test_no_licence_key_in_a_binding.py",
}

# One pattern per way a key came back into the product, with the name of the
# thing it is. The message is half the guard: a hit that only says "forbidden
# pattern" sends the reader looking for a rule instead of at the code.
PATRONEN: list[tuple[str, re.Pattern[str], str]] = [
    (
        "key entry point",
        re.compile(
            r"\b(?:set_license_key|activate_license|setLicenseKey|activateLicenseKey"
            r"|set_license_payload|setLicensePayload|set_license_public_key"
            r"|setLicensePublicKey|pdfluent_license_activate_key"
            r"|pdfluent_license_activate_file|pdfluent_license_activate_payload"
            r"|pdfluent_license_set_public_key|nativeActivateKey|nativeActivatePayload"
            r"|ActivateKey)\b"
        ),
        "a caller can hand the SDK a licence key again",
    ),
    (
        "environment key",
        re.compile(r"PDFLUENT_LICENSE_(?:KEY|FILE)"),
        "the environment decides what the SDK will do",
    ),
    (
        "capability gate",
        re.compile(
            r"\b(?:require_capability|require_capability_with_override"
            r"|CapabilityNotLicensed|ErrorCapabilityNotLicensed"
            r"|CAPABILITY_NOT_LICENSED|FeatureNotInTier|effective_tier"
            r"|nativeEffectiveTier|pdfluent_license_effective_tier)\b"
        ),
        "work is refused because of a tier",
    ),
    (
        "output mark",
        re.compile(r"PDFluent Free Tier|PDFluent trial|output_is_marked|outputIsMarked"),
        "output is stamped because no key was present",
    ),
]

# FLOOR: the scoped tree holds well over a thousand source files. `git ls-files`
# run outside a repository prints nothing and exits 0, and a scan over no files
# reports a clean product in the same words a real scan uses. That silence is
# the failure this number exists to catch.
MINIMUM_BESTANDEN = 400

# Enough of a file to tell text from binary. Fixtures and compiled artefacts
# carry no source to read.
KOP_BYTES = 8192


def _git_env() -> dict[str, str]:
    """git without the caller's repository-location variables.

    Inside a hook GIT_DIR and GIT_WORK_TREE are absolute and inherited, so every
    subprocess would work on the caller's repository instead of this one.
    """
    weg = {
        "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES", "GIT_PREFIX",
    }
    return {k: v for k, v in os.environ.items() if k not in weg}


def tracked_files(root: str = ".") -> list[str]:
    r = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, capture_output=True, env=_git_env()
    )
    if r.returncode != 0:
        return []
    return [p.decode("utf-8", "replace") for p in r.stdout.split(b"\0") if p]


def in_scope(pad: str) -> bool:
    if pad in VRIJGESTELD:
        return False
    if not pad.startswith(SCOPE):
        return False
    if TESTPADEN.search(pad):
        return False
    return pad.endswith(EXTENSIES)


def shipped_part(tekst: str) -> str:
    """The text before the first inline test module."""
    m = CFG_TEST.search(tekst)
    return tekst if m is None else tekst[: m.start()]


def scan(root: str = ".", floor: int = MINIMUM_BESTANDEN):
    """(files read, hits). A hit is (path, line number, kind, line, why)."""
    gelezen = 0
    treffers: list[tuple[str, int, str, str, str]] = []

    for pad in tracked_files(root):
        if not in_scope(pad):
            continue
        vol = os.path.join(root, pad)
        try:
            with open(vol, "rb") as f:
                kop = f.read(KOP_BYTES)
                if b"\0" in kop:
                    continue
                ruw = kop + f.read()
        except OSError:
            # A symlink to nowhere, or a path removed between ls-files and here.
            continue
        gelezen += 1
        tekst = shipped_part(ruw.decode("utf-8", "replace"))
        for nr, regel in enumerate(tekst.splitlines(), start=1):
            for soort, patroon, waarom in PATRONEN:
                if patroon.search(regel):
                    treffers.append((pad, nr, soort, regel.strip()[:120], waarom))

    if gelezen < floor:
        raise SystemExit(
            f"[no-key] FATAL: {gelezen} source file(s) read, below the floor of "
            f"{floor}.\nA scan over nothing reports a clean product in the same "
            "words as a real one.\nUsually the working directory is not the "
            "repository root, or the checkout is empty."
        )
    return gelezen, treffers


def main() -> int:
    if not PATRONEN:
        print(
            "[no-key] FATAL: no patterns, so this guard accepts everything.",
            file=sys.stderr,
        )
        return 1

    gelezen, treffers = scan()

    if treffers:
        print(
            f"[no-key] {len(treffers)} line(s) put a licence check back into the "
            "product:\n",
            file=sys.stderr,
        )
        for pad, nr, soort, regel, waarom in treffers[:40]:
            print(f"  {pad}:{nr}  [{soort}] {waarom}", file=sys.stderr)
            print(f"      {regel}", file=sys.stderr)
        if len(treffers) > 40:
            print(f"  ... and {len(treffers) - 40} more", file=sys.stderr)
        print(
            "\nThere is no licence key (#199, #226). LICENSE-COMMERCIAL §5 says so\n"
            "as a statement of fact about this binary, and docs/licensing.md tells a\n"
            "buyer the commercial route is a signed order form and nothing else.\n"
            "If the decision has changed, that change belongs in those two documents\n"
            "first, in its own commit, and this guard is edited with them.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[no-key] OK: {gelezen} source file(s) in {len(SCOPE)} tree(s) read, "
        f"{len(PATRONEN)} pattern(s) checked, no licence check present."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
