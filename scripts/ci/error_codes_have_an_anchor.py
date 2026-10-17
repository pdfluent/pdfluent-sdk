#!/usr/bin/env python3
"""Every docs_url() must point at a section that exists (#253).

`docs_url()` used to emit https://pdfluent.com/errors/<code>, one page per
code. None of those pages were ever published: each returned 404, so the one
link we hand a user at the moment something breaks led nowhere.

The published page is an index with an anchor per code. Pointing at the anchor
costs nothing and works today.

This checks the codes against the anchors. It reads the page over the network
when it can and announces a skip when it cannot, because a link check that
quietly passes offline is not a link check.
"""
from __future__ import annotations

import pathlib
import re
import sys
import urllib.error
import urllib.request

WORTEL = pathlib.Path(__file__).resolve().parents[2]
BRON = WORTEL / "crates/pdfluent/src/error.rs"
PAGINA = "https://pdfluent.com/errors/"

# Codes the library can return that the published page has no section for.
# Recorded rather than removed: error.rs states an append-only policy for codes
# ("You must never: Remove a code"), so the fix belongs on the website side.
# Named individually so the list cannot grow without somebody deciding to.
GEEN_ANKER: dict[str, str] = {
    # A code goes in here only with the issue that will publish its section.
    # Was empty from 03-09-2026 (#320 gave both licence codes theirs); #414 put
    # E-PAGE-OUT-OF-RANGE here against #426, and #426 has since published the
    # section, so it came back out on 08-09-2026. Empty is the resting state:
    # a register that keeps entries after they are fixed stops being read.
    #
    # #426 also closed the way the entry got here. The page's input is
    # generated from error.rs now
    # (scripts/docs/generate_error_catalogue.py writes docs/sdk-error-codes.json,
    # scripts/ci/error_catalogue_is_generated.py refuses a stale copy), so a
    # code without a section is a red gate rather than a line in this dict.
    #
    # If you are about to add one anyway, check first whether the section can
    # simply be published instead. Every entry this dict has held so far was a
    # code nobody had got round to, not a code that needed a decision.
}


def main() -> int:
    if not BRON.exists():
        print(f"FAIL: {BRON.relative_to(WORTEL)} is missing", file=sys.stderr)
        return 1

    codes = set(re.findall(r"pdfluent\.com/errors#([A-Z0-9-]+)", BRON.read_text()))
    if not codes:
        print("FAIL: no anchor-form documentation URLs found in error.rs. Either the "
              "URLs went back to the per-page form, which 404s, or the helper is gone.",
              file=sys.stderr)
        return 1

    try:
        # Cloudflare answers a bare urllib with 403; curl gets through. A
        # user-agent is not a workaround here, it is the minimum politeness.
        verzoek = urllib.request.Request(
            PAGINA, headers={"User-Agent": "pdfluent-ci-link-check"}
        )
        with urllib.request.urlopen(verzoek, timeout=20) as antwoord:
            pagina = antwoord.read().decode("utf-8", "replace")
    except (urllib.error.URLError, OSError) as fout:
        print(f"SKIPPED (not a pass): could not fetch {PAGINA}: {fout}", file=sys.stderr)
        return 0

    ankers = set(re.findall(r'id="([A-Z0-9-]+)"', pagina))
    ontbreekt = {c for c in codes if c not in ankers}
    nieuw = ontbreekt - set(GEEN_ANKER)
    opgelost = set(GEEN_ANKER) - ontbreekt

    print(f"[error-anchors] {len(codes)} code(s) in error.rs, {len(ankers)} anchor(s) "
          f"on the page, {len(GEEN_ANKER)} known gap(s)")

    if nieuw:
        print(f"\nFAIL: {len(nieuw)} code(s) link to a section that does not exist: "
              f"{', '.join(sorted(nieuw))}. A user hits that link at the moment "
              "something has already gone wrong; landing nowhere is the second failure.",
              file=sys.stderr)
        return 1

    if opgelost:
        # ADVISORY, not a failure. A code with a section is the outcome this
        # guard exists to produce; the stale GEEN_ANKER line is bookkeeping.
        # Refusing on it makes a website deploy turn every landing on master
        # red -- which is what happened when #426's page went live, on a repo
        # nobody had pushed to. A guard that changes colour because someone
        # else deployed a site must not hold a merge.
        print(f"[error-anchors] NOTE: {len(opgelost)} code(s) now have a section and "
              f"are still listed as a known gap: {', '.join(sorted(opgelost))}. Remove "
              "them from GEEN_ANKER so the next missing one is noticed.")

    print("[error-anchors] OK: every code points at a section that exists, or at a "
          "recorded gap.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
