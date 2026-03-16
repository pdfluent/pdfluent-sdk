#!/usr/bin/env python3
"""
xfa-pdf — demo of core PDF operations.

Run:
    cd crates/pdf-python
    maturin develop
    python examples/demo.py path/to/document.pdf [path/to/acroform.pdf]
"""

import sys
import os
import tempfile

try:
    from xfa_pdf import (
        Document,
        open_pdf,
        merge_pdfs,
        validate_pdfa,
        decrypt_pdf,
    )
except ImportError:
    print("xfa_pdf not installed. Run: maturin develop")
    sys.exit(1)


def main(pdf_path: str) -> None:
    print(f"\n=== xfa_pdf demo ===\nFile: {pdf_path}\n")

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

    # ------------------------------------------------------------------ #
    # 8. get_form_fields / set_form_field                                 #
    # ------------------------------------------------------------------ #
    print("\n8. get_form_fields() / set_form_field()")
    if len(sys.argv) >= 3:
        acroform_path = sys.argv[2]
        form_doc = Document(acroform_path)
        fields = form_doc.get_form_fields()
        print(f"   {len(fields)} fields found")
        for f in fields[:5]:
            print(f"   [{f.field_type}] {f.name!r} = {f.value!r}  (page {f.page})")
        if fields:
            text_fields = [f for f in fields if f.field_type == "text"]
            if text_fields:
                name = text_fields[0].name
                ok = form_doc.set_form_field(name, "Hello from xfa_pdf")
                print(f"   set_form_field({name!r}) → {ok}")
                with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as tf:
                    filled_path = tf.name
                form_doc.save(filled_path)
                print(f"   saved filled form to {filled_path}")
                os.unlink(filled_path)
    else:
        print("   (pass a second path to an AcroForm PDF to demo form fields)")

    # ------------------------------------------------------------------ #
    # 9. get_annotations / add_annotation                                 #
    # ------------------------------------------------------------------ #
    print("\n9. add_annotation() / get_annotations()")
    annot_doc = Document(pdf_path)
    annot_doc.add_annotation(0, "highlight", (72.0, 700.0, 300.0, 720.0), "Demo highlight")
    annot_doc.add_annotation(0, "freetext", (72.0, 650.0, 300.0, 680.0), "Demo note")
    annots_before = annot_doc.get_annotations(0)
    print(f"   annotations on page 0: {len(annots_before)}")
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as tf:
        annot_path = tf.name
    annot_doc.save(annot_path)
    # Read back
    reloaded = Document(annot_path)
    annots_after = reloaded.get_annotations(0)
    print(f"   after save+reload: {len(annots_after)} annotations")
    for a in annots_after:
        print(f"   [{a.annot_type}] rect={a.rect}  contents={a.contents!r}")
    os.unlink(annot_path)

    # ------------------------------------------------------------------ #
    # 10. redact_text                                                      #
    # ------------------------------------------------------------------ #
    print("\n10. redact_text()")
    redact_doc = Document(pdf_path)
    report = redact_doc.redact_text("the")
    print(
        f"   matches={report.matches_found}  redacted={report.areas_redacted}"
        f"  pages={report.pages_affected}"
    )
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as tf:
        redacted_path = tf.name
    redact_doc.save(redacted_path)
    print(f"   redacted PDF saved to {redacted_path}")
    os.unlink(redacted_path)

    # ------------------------------------------------------------------ #
    # 11. encrypt / decrypt                                                #
    # ------------------------------------------------------------------ #
    print("\n11. encrypt() / decrypt()")
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as tf:
        enc_path = tf.name
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as tf:
        dec_path = tf.name
    doc.encrypt(enc_path, "secret123")
    print(f"   encrypted → {enc_path}")
    decrypt_pdf(enc_path, dec_path, "secret123")
    dec_doc = Document(dec_path)
    print(f"   decrypted → {dec_path}  ({dec_doc.page_count} pages)")
    os.unlink(enc_path)
    os.unlink(dec_path)

    print("\nDemo complete.")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <path-to-pdf> [path-to-acroform-pdf]")
        sys.exit(1)
    main(sys.argv[1])
