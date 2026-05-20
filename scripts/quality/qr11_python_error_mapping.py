#!/usr/bin/env python3
"""QR-11 Python runtime error-mapping. Proves the LOCAL PDFluent Python
binding maps canonical error cases to its typed exception hierarchy.

Anti-false-green guards (a same-named conda/PyPI `pdfluent` must NOT count):
  * `pdfluent.__file__` must live inside this repo's pdf-python tree (or a
    build venv), and
  * the typed hierarchy (`PdfluentError` + `PdfluentParseError`) and the
    `Document`/`open_pdf` API must exist — the placeholder package has none.

Run with the binding's venv python:
  /tmp/q11_pyvenv/bin/python3 scripts/quality/qr11_python_error_mapping.py
Exit 0 = green; 2 = SKIP (binding not present/identity fail); 1 = mapping fail.
"""
from __future__ import annotations
import os, sys, tempfile

try:
    import pdfluent
except Exception as e:  # noqa: BLE001
    print(f"SKIP pdfluent not importable: {e}"); sys.exit(2)

# --- identity guards ---
f = getattr(pdfluent, "__file__", "") or ""
if "pdf-python" not in f and "q11_pyvenv" not in f:
    print(f"SKIP not the local PDFluent binding (file={f})"); sys.exit(2)
for attr in ("Document", "open_pdf", "PdfluentError", "PdfluentParseError"):
    if not hasattr(pdfluent, attr):
        print(f"SKIP missing real-binding attribute: {attr} (placeholder package?)"); sys.exit(2)
print(f"identity OK: {f}")

PdfluentError = pdfluent.PdfluentError
fails, observed = [], {}

def expect_typed(label, fn):
    try:
        fn(); fails.append(f"{label}: no error raised (silent success)"); observed[label] = "NO_RAISE"
    except PdfluentError as e:               # typed binding error -> good
        observed[label] = type(e).__name__
    except Exception as e:                    # noqa: BLE001  untyped -> bad
        observed[label] = "UNTYPED:" + type(e).__name__
        fails.append(f"{label}: untyped {type(e).__name__} (expected a PdfluentError subclass)")

# valid control -> must succeed (discrimination)
try:
    d = pdfluent.open_pdf("tests/corpus-mini/multi-page.pdf")
    # page_count is a property (pyo3 getter), not a method.
    pages = d.page_count() if callable(getattr(type(d), "page_count", None)) else d.page_count
    observed["valid_control"] = f"pages={pages}"
    if pages < 1:
        fails.append("valid_control: pages < 1")
except Exception as e:  # noqa: BLE001
    fails.append(f"valid_control: valid PDF failed to open: {e}")

# malformed bytes -> typed error
with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as t:
    t.write(b"%PDF-1.7\nnot a real pdf \xde\xad\xbe\xef"); bad = t.name
expect_typed("malformed", lambda: pdfluent.open_pdf(bad))
# empty file -> typed error
with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as t:
    empty = t.name
expect_typed("empty", lambda: pdfluent.open_pdf(empty))
# nonexistent path -> typed error
expect_typed("missing_path", lambda: pdfluent.open_pdf("/no/such/file_qr11.pdf"))

for k, v in observed.items():
    print(f"  {k}: {v}")
os.unlink(bad); os.unlink(empty)
if fails:
    print("FAIL:", "; ".join(fails), file=sys.stderr); sys.exit(1)
print("QR-11 python error mapping: OK"); sys.exit(0)
