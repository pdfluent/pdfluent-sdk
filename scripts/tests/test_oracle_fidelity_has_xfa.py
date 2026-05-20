#!/usr/bin/env python3
"""Regression tests for xfa_adobe_oracle_fidelity_compare.has_xfa().

Guards the structural-detection correction: a flattened PDF that is structurally
clean (no /AcroForm, no /XFA key) must NOT be flagged as XFA-retained even when
its compressed streams happen to contain the coincidental byte sequence b"/XFA".
Conversely, a PDF carrying a real /XFA in its AcroForm MUST be flagged.

Empirical context: on the pdfRest oracle set the old raw-byte heuristic produced
89 false positives (90 flagged vs 1 truly structural). See
benchmarks/runs/xfa_enterprise_plan/adobe_oracle_fidelity/PHASE2_ORACLE_BATCH_RESULTS.md.

Skips gracefully if pikepdf is unavailable (the function then uses a decompressed
xdp:xdp fallback, which these synthetic fixtures also satisfy).
"""
import importlib.util
import sys
import zlib
from pathlib import Path

import pytest

HARNESS = Path(__file__).resolve().parents[1] / "xfa_adobe_oracle_fidelity_compare.py"
spec = importlib.util.spec_from_file_location("oracle_cmp", HARNESS)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

pikepdf = pytest.importorskip("pikepdf")


def _clean_pdf_with_coincidental_xfa_bytes(path: Path):
    """Structurally clean PDF whose compressed stream decompresses WITHOUT /XFA
    but whose raw bytes contain b"/XFA" (mimicking compressed-stream coincidence)."""
    pdf = pikepdf.new()
    pdf.add_blank_page(page_size=(200, 200))
    # Content stream with no /XFA after decompression.
    pdf.save(path)
    # Append a coincidental b"/XFA" byte run outside any structural dictionary.
    with open(path, "ab") as fh:
        fh.write(b"\n%% coincidental /XFA bytes in trailer comment\n")


def _pdf_with_real_acroform_xfa(path: Path):
    pdf = pikepdf.new()
    pdf.add_blank_page(page_size=(200, 200))
    packet = pdf.make_stream(zlib.compress(b"<xdp:xdp>...</xdp:xdp>"))
    af = pdf.make_indirect(pikepdf.Dictionary(XFA=pikepdf.Array([pikepdf.String("template"), packet])))
    pdf.Root.AcroForm = af
    pdf.Root.NeedsRendering = True
    pdf.save(path)


def test_clean_pdf_with_coincidental_xfa_bytes_not_flagged(tmp_path):
    p = tmp_path / "clean.pdf"
    _clean_pdf_with_coincidental_xfa_bytes(p)
    raw = p.read_bytes()
    assert b"/XFA" in raw, "fixture must contain coincidental raw /XFA bytes"
    assert mod.has_xfa(str(p)) is False, "structurally clean PDF must not be flagged"


def test_real_acroform_xfa_is_flagged(tmp_path):
    p = tmp_path / "xfa.pdf"
    _pdf_with_real_acroform_xfa(p)
    assert mod.has_xfa(str(p)) is True, "PDF with real /XFA in AcroForm must be flagged"


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
