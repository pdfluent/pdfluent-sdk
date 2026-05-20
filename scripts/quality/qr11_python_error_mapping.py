#!/usr/bin/env python3
"""QR-11 Python runtime error-mapping smoke. Runs only when the `pdfluent`
Python binding is importable. Asserts canonical error cases raise typed
exceptions (not bare RuntimeError/None)."""
import sys
try:
    import pdfluent
except Exception as e:  # noqa: BLE001
    print(f"SKIP pdfluent not importable: {e}"); sys.exit(0)

fails = []
# malformed PDF -> typed error
try:
    pdfluent.PdfDocument.from_bytes(b"%PDF-1.7\nnot a real pdf")
    fails.append("malformed did not raise")
except Exception as e:  # expected
    if e.__class__.__name__ == "RuntimeError":
        fails.append("malformed raised bare RuntimeError (untyped)")
if fails:
    print("FAIL:", fails); sys.exit(1)
print("OK python error mapping"); sys.exit(0)
