#!/usr/bin/env python3
"""
pdfengine — demo of core PDF operations.

Run:
    cd crates/pdf-python
    maturin develop
    python examples/demo.py path/to/document.pdf
"""

import sys
import os
import tempfile

try:
    from pdfengine import (
        Document,
        open_pdf,
        merge_pdfs,
        validate_pdfa,
    )
except ImportError:
    print("pdfengine not installed. Run: maturin develop")
    sys.exit(1)


def main(pdf_path: str) -> None:
    print(f"\n=== pdfengine demo ===\nFile: {pdf_path}\n")

    # ------------------------------------------------------------------ #
    # 1. open_pdf() convenience function                                   #
    # ------------------------------------------------------------------ #
    print("1. open_pdf()")
    doc = open_pdf(pdf_path)
    print(f"   {doc}")

    # ------------------------------------------------------------------ #
    # 2. page_count                                                        #
    # ------------------------------------------------------------------ #
    print(f"\n2. page_count: {doc.page_count}")

    # ------------------------------------------------------------------ #
    # 3. extract_text — per page via Document.extract_text(page_num)      #
    # ------------------------------------------------------------------ #
    print("\n3. extract_text(page_num)")
    for i in range(min(doc.page_count, 3)):
        text = doc.extract_text(i)
        preview = text[:120].replace("\n", " ") if text else "(no text)"
        print(f"   page {i}: {preview!r}")

    # Alternative: page-level extract_text()
    print("\n   (also available as doc[0].extract_text())")
    print(f"   page 0: {doc[0].extract_text()[:80]!r}")

    # ------------------------------------------------------------------ #
    # 4. save() — write a copy                                            #
    # ------------------------------------------------------------------ #
    print("\n4. document.save()")
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
        copy_path = f.name
    doc.save(copy_path)
    copy = Document(copy_path)
    print(f"   saved copy: {copy_path}  ({copy.page_count} pages)")
    os.unlink(copy_path)

    # ------------------------------------------------------------------ #
    # 5. merge_pdfs()                                                      #
    # ------------------------------------------------------------------ #
    print("\n5. merge_pdfs()")
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
        merged_path = f.name
    merge_pdfs([pdf_path, pdf_path], merged_path)
    merged = Document(merged_path)
    print(
        f"   merged {doc.page_count} + {doc.page_count} pages "
        f"→ {merged.page_count} pages at {merged_path}"
    )
    os.unlink(merged_path)

    # ------------------------------------------------------------------ #
    # 6. validate_pdfa()                                                   #
    # ------------------------------------------------------------------ #
    print("\n6. validate_pdfa()")
    report = validate_pdfa(pdf_path)
    status = "PASS" if report.is_compliant else "FAIL"
    level = report.pdfa_level or "unknown"
    print(
        f"   level: {level}  status: {status}  "
        f"errors: {report.error_count}  warnings: {report.warning_count}"
    )
    for issue in report.issues[:5]:
        print(f"   [{issue.severity}] {issue.rule}: {issue.message}")
    if len(report.issues) > 5:
        print(f"   ... and {len(report.issues) - 5} more issues")

    # ------------------------------------------------------------------ #
    # 7. metadata                                                          #
    # ------------------------------------------------------------------ #
    print("\n7. metadata")
    meta = doc.metadata
    print(f"   title:    {meta.title!r}")
    print(f"   author:   {meta.author!r}")
    print(f"   producer: {meta.producer!r}")

    print("\nDemo complete.")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <path-to-pdf>")
        sys.exit(1)
    main(sys.argv[1])
