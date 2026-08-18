#!/usr/bin/env python3
"""Does every feature we advertise have a test that proves it?

WHY THIS EXISTS

api_coverage.py answers "is every exported symbol called by a test". That is the
binding question. This is the product question: for every capability we tell
customers we have, is there a test that exercises it end to end?

They are not the same. A symbol can be called by a test that asserts nothing
useful, and a feature can be advertised without any single symbol obviously
belonging to it — "Organize PDF pages" is reorder, rotate and delete together.

WHAT PROMPTED IT

The advertised list was checked by hand on 2026-08-18 and every feature did have
tests. The problem was elsewhere and worse: quality:cargo-test was manual on
merge requests and on master, automatic only on a schedule, and no schedule
existed. 202 test binaries — including all of these features — were running on
nothing. The tests were fine; nothing executed them.

So this file exists to keep the *mapping* honest once the suite runs again. If a
feature loses its test, or a test is renamed away, this says so by name instead
of the coverage total drifting down by a fraction nobody notices.

MAINTAINING THE LIST

`PROMISES` mirrors what the website and the editor tell customers they can do.
When a feature is added there, add it here in the same change — that is the
Definition of Done in CLAUDE.md applied to product copy rather than to code.

Exit codes:
    0  every promise maps to at least one test that exists
    1  a promise has no test, or names one that is gone
    2  could not run
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# feature -> (what the customer is told, implementing crates, symbols)
#
# The crate list is what keeps this honest. Searching the whole workspace for
# `merge(` matched lopdf's reader, which has nothing to do with merging
# documents — a false pass. Scoping each promise to the crates that actually
# implement it removes that, and forces the mapping to be stated rather than
# guessed.
PROMISES: dict[str, tuple[str, list[str], list[str]]] = {
    "Merge PDF": (
        "Combine several PDFs into one document in the order you choose",
        ["pdf-manip", "pdfluent", "pdf-capi", "xfa-wasm", "pdf-node", "pdf-python"],
        ["merge_pdfs", "merge_documents", "merge"],
    ),
    "Split PDF": (
        "Divide a document by page range to create separate files",
        ["pdf-manip", "pdfluent", "pdf-capi", "pdf-node", "pdf-python"],
        ["split_pdf", "split_range", "extract_pages", "split"],
    ),
    "Organize PDF pages": (
        "Reorder, rotate, or delete pages without leaving the app",
        ["pdf-manip", "pdfluent", "pdf-node", "pdf-python"],
        ["rotate_pages", "rotate", "delete_pages", "reorder_pages", "remove_pages"],
    ),
    "Compress PDF": (
        "Reduce file size",
        ["pdf-manip", "pdfluent", "pdf-capi", "pdf-node", "pdf-python"],
        ["compress"],
    ),
    "Password protect PDF": (
        "Set a password and restrict permissions on a document",
        ["pdf-manip", "pdfluent", "pdf-node", "pdf-python"],
        ["encrypt", "decrypt"],
    ),
    "Add watermark to PDF": (
        "Add a text watermark across all pages",
        ["pdf-manip", "pdfluent", "pdf-capi", "pdf-node", "pdf-python"],
        ["add_watermark", "watermark"],
    ),
    "PDF to Word": (
        "Convert PDF to Word",
        ["pdf-docx", "pdfluent"],
        ["pdf_to_docx", "convert_pdf_bytes_to_docx", "to_docx"],
    ),
    "PDF to Excel": (
        "Convert PDF to Excel",
        ["pdf-xlsx", "pdfluent"],
        ["pdf_to_xlsx", "convert_pdf_bytes_to_xlsx", "extract_tables"],
    ),
    "PDF to PowerPoint": (
        "Convert PDF to PowerPoint",
        ["pdf-pptx", "pdfluent"],
        ["pdf_to_pptx", "convert_pdf_bytes_to_pptx"],
    ),
    "PDF to image": (
        "Convert PDF to image",
        ["pdf-render", "pdf-engine", "pdfluent", "xfa-wasm"],
        ["render_page", "render_thumbnail", "render_to_image"],
    ),
    "PDF to PDF/A": (
        "Convert PDF to PDF/A",
        ["pdf-manip", "pdf-compliance", "pdfluent", "xfa-wasm"],
        ["convert_to_pdfa", "convert_bytes", "validate_pdfa"],
    ),
}


def test_sources() -> dict[Path, str]:
    out = {}
    for pattern in ("crates/*/tests/*.rs", "crates/*/src/**/*.rs"):
        for f in REPO.glob(pattern):
            if "/target/" in str(f):
                continue
            out[f] = f.read_text(errors="replace")
    return out


def main() -> None:
    sources = test_sources()
    if not sources:
        print("[feature_promises] FATAL: geen bronbestanden gevonden", file=sys.stderr)
        sys.exit(2)

    # Only count a hit inside a test: an implementation calling itself is not
    # evidence of coverage.
    test_only = {
        f: t for f, t in sources.items()
        if "/tests/" in str(f) or "#[test]" in t or "#[wasm_bindgen_test]" in t
    }

    print("=" * 72)
    print("[feature_promises] Heeft elke geadverteerde functie een test?")
    print("=" * 72)

    missing: list[str] = []
    for feature, (blurb, crates, symbols) in PROMISES.items():
        hits: list[str] = []
        for f, text in test_only.items():
            rel = f.relative_to(REPO)
            if not any(part == c for c in crates for part in rel.parts):
                continue
            for sym in symbols:
                if re.search(rf"\b{re.escape(sym)}\s*\(", text):
                    hits.append(str(rel))
                    break
        hits = sorted(set(hits))
        if hits:
            print(f"  OK   {feature:22} {len(hits)} testbestand(en)  bv. {hits[0]}")
        else:
            print(f"  GAP  {feature:22} geen test in {crates} vindt {symbols}")
            missing.append(feature)

    # Second axis: a promise can have tests in a crate a customer cannot reach.
    # OCR, Excel and PowerPoint all have tests in their own crates while the
    # `pdfluent` facade does not depend on them at all — so the SDK claim on the
    # features page is not met even though the test count looks fine. Coverage
    # without reachability is not a kept promise.
    facade = subprocess.run(
        ["cargo", "tree", "-p", "pdfluent", "--depth", "1", "--edges", "normal"],
        capture_output=True, text=True, cwd=REPO,
    ).stdout
    unreachable = []
    for feature, (blurb, crates, symbols) in PROMISES.items():
        impl = [c for c in crates if c != "pdfluent"]
        if impl and not any(c in facade for c in impl):
            unreachable.append((feature, impl))

    if unreachable:
        print("-" * 72)
        print("  NIET BEREIKBAAR vanuit de pdfluent-facade (wel getest, wel gepubliceerd):")
        for feature, impl in unreachable:
            print(f"    {feature:22} zit alleen in {impl}")
        print("  Een klant die de facade gebruikt, krijgt deze niet zonder de crate")
        print("  er handmatig bij te zetten. Zie docs/SYSTEM_MAP.md.")

    print("-" * 72)
    print(f"  {len(PROMISES) - len(missing)}/{len(PROMISES)} beloften hebben een test")
    print(f"  {len(PROMISES) - len(unreachable)}/{len(PROMISES)} beloften zijn bereikbaar vanuit de facade")
    print()
    print("  Let op: dit toetst dat er een test BESTAAT die de functie aanraakt.")
    print("  Of die test iets zinnigs beweert, en of hij in CI draait, zijn")
    print("  aparte vragen — zie de Definition of Done in CLAUDE.md.")

    sys.exit(1 if (missing or unreachable) else 0)


if __name__ == "__main__":
    main()
