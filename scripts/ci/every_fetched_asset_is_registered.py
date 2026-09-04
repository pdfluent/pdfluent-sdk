#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every asset this software downloads has a row in docs/FETCHED_ASSETS.toml.

#214 asks for a licence check that covers every ecosystem. `license_gate.py`
covers the ones with a manifest -- Cargo, npm, Maven, NuGet, PyPI. A model
fetched at run time has no manifest, so it walks past all five, and a set of
trained weights is exactly the kind of third-party work whose licence is not the
licence of the code that fetches it.

WHAT COUNTS AS A FETCH, AND WHY NOT EVERY URL
=============================================
There are 406 URLs in `crates/*/src`. Almost all are XML namespaces and
specification references -- `http://www.xfa.org/schema/...` is an identifier, not
a place bytes come from. Demanding a licence row for those would bury the two
that matter.

So a URL counts when it is a HOST WE TAKE BYTES FROM: the register lists those
prefixes, and this guard checks the other direction -- that no source file names
a download host which the register does not carry. The list of hosts to look for
comes from the register itself plus a small set of shapes that mean "artefact
store" rather than "identifier".

THIS GUARD DOES NOT JUDGE A LICENCE
===================================
It checks that a fetch is registered and that its row says what the source
states. Whether "no licence stated, trained on CC-BY-SA data" is acceptable to
ship is an owner's decision, and a guard that answered it would be pretending to
a judgement it cannot make. What it can do is make sure the question is written
down where it cannot be lost.
"""
from __future__ import annotations
import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REGISTER = REPO / "docs" / "FETCHED_ASSETS.toml"
BRON = REPO / "crates"

# Hosts that serve artefacts rather than identify a schema. A namespace URL is
# never fetched; these are.
ARTEFACT_HOST = re.compile(
    r"https?://("
    r"[^/\s\"']*\.s3[.-][^/\s\"']*amazonaws\.com"      # S3, any regional spelling
    r"|huggingface\.co/[^/\s\"']+/[^/\s\"']+/resolve"  # HF model files
    r"|[^/\s\"']*\.blob\.core\.windows\.net"
    r"|storage\.googleapis\.com"
    r"|[^/\s\"']*/releases/download"                   # GitHub release assets
    r")[^\s\"'`)>]*"
)

VERPLICHT = ("url_prefix", "what", "used_by", "licence", "licence_source",
             "verified", "decision")


def rows() -> list[dict]:
    if not REGISTER.is_file():
        raise SystemExit(
            "every_fetched_asset_is_registered: SKIPPED (not a pass) -- "
            f"{REGISTER.relative_to(REPO)} is missing, so nothing was checked."
        )
    return tomllib.loads(REGISTER.read_text(encoding="utf-8")).get("asset", [])


def main() -> int:
    register = rows()
    problems: list[str] = []

    for i, row in enumerate(register, 1):
        missing = [k for k in VERPLICHT if not str(row.get(k, "")).strip()]
        if missing:
            problems.append(f"row {i} ({row.get('url_prefix', '?')}) has no "
                            + ", ".join(missing))

    prefixes = [r["url_prefix"] for r in register if r.get("url_prefix")]

    seen: dict[str, str] = {}
    files = 0
    for f in sorted(BRON.rglob("*.rs")):
        if "target" in f.parts:
            continue
        files += 1
        for m in ARTEFACT_HOST.finditer(f.read_text(errors="replace")):
            url = m.group(0).rstrip(".,;:`\\")
            if not any(url.startswith(p) for p in prefixes):
                seen.setdefault(url, str(f.relative_to(REPO)))

    # A floor on what was examined, not on what was found. Zero files means the
    # tree moved or the glob broke, and reporting OK over nothing examined is the
    # failure these guards exist to refuse.
    if files < 50:
        print(f"[fetched-assets] SKIPPED (not a pass): only {files} source "
              "file(s) examined, which cannot be right. The search is broken.",
              file=sys.stderr)
        return 1

    for url, where in sorted(seen.items()):
        problems.append(f"{where} fetches {url}, which no row covers")

    if problems:
        print(f"[fetched-assets] {len(problems)} problem(s):\n", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("\n  Add a row to docs/FETCHED_ASSETS.toml saying what the source\n"
              "  states about the licence. If it states nothing, that is what the\n"
              "  row says -- this register records, it does not decide.",
              file=sys.stderr)
        return 1

    print(f"[fetched-assets] OK: {len(register)} registered asset(s), "
          f"{files} source file(s) examined, no unregistered fetch.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
