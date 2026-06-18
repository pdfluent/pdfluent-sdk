"""Tests for the digital-signature verification surface (GA parity).

Covers the new ``Document`` methods mirroring the Rust core
``PdfDocument::verify_signatures`` / ``PdfDocument::signatures`` and the Node
``validateSignatures()`` parity reference:

    Document.validate_signatures() -> list[SignatureResult]
    Document.verify_signatures()   -> list[SignatureResult]   (alias)
    Document.signatures()          -> list[SignatureResult]   (metadata only)

Plus a smoke check that the signed-payload license functions are importable
and callable from the top-level ``pdfluent`` package.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_signatures.py -v
"""

from __future__ import annotations

import os

import pytest

pdfluent = pytest.importorskip("pdfluent")
from pdfluent import Document, SignatureResult  # noqa: E402

# Fixtures (symlinked into ../../../fixtures by the corpus-mini setup).
FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
SAMPLE_PDF = os.path.join(FIXTURES, "sample.pdf")  # unsigned
SIGNED_PDF = os.path.join(FIXTURES, "signed.pdf")  # signed-rsa.pdf

# Repo-root corpus PDF that surfaces a discoverable, *valid* signature field
# (Signature1 / "Test User"). Used for a positive end-to-end assertion of the
# validated result shape; skipped when the corpus is not checked out.
_REPO_ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "..")
SIGNED_CORPUS_PDF = os.path.join(_REPO_ROOT, "tests", "regression-388", "redact_0626.pdf")


# ---------------------------------------------------------------------------
# Unsigned document — must return an empty list, never raise.
# ---------------------------------------------------------------------------


class TestUnsignedDocument:
    def test_validate_signatures_empty(self) -> None:
        doc = Document(SAMPLE_PDF)
        results = doc.validate_signatures()
        assert isinstance(results, list)
        assert len(results) == 0

    def test_verify_signatures_alias_empty(self) -> None:
        doc = Document(SAMPLE_PDF)
        assert doc.verify_signatures() == []

    def test_signatures_metadata_empty(self) -> None:
        doc = Document(SAMPLE_PDF)
        results = doc.signatures()
        assert isinstance(results, list)
        assert len(results) == 0

    def test_validate_does_not_raise_on_plain_pdf(self) -> None:
        # Contract: unsigned document yields zero results without an exception.
        doc = Document(SAMPLE_PDF)
        try:
            doc.validate_signatures()
        except Exception as exc:  # pragma: no cover - defensive
            pytest.fail(f"validate_signatures raised on unsigned PDF: {exc!r}")


# ---------------------------------------------------------------------------
# Signed document — exercise the parity contract end-to-end (if the fixture is
# present). The minimal corpus fixture may not surface discoverable signature
# fields via the read-only parser, so — exactly like the Node parity smoke
# test (`expect(Array.isArray(sigs)).toBe(true)`) — the universal contract is
# "returns a list, never raises"; per-result shape is asserted only when the
# document actually surfaces signatures.
# ---------------------------------------------------------------------------


@pytest.mark.skipif(
    not os.path.exists(SIGNED_PDF), reason="signed.pdf fixture not available"
)
class TestSignedDocument:
    def test_validate_signatures_returns_list(self) -> None:
        doc = Document(SIGNED_PDF)
        results = doc.validate_signatures()
        assert isinstance(results, list)

    def test_result_shape(self) -> None:
        doc = Document(SIGNED_PDF)
        for r in doc.validate_signatures():
            assert isinstance(r, SignatureResult)
            assert isinstance(r.status, str)
            assert r.status in ("valid", "invalid", "unknown")
            assert r.reason is None or isinstance(r.reason, str)
            assert isinstance(r.field_name, str)
            assert r.signer is None or isinstance(r.signer, str)
            assert r.timestamp is None or isinstance(r.timestamp, str)
            assert "SignatureResult" in repr(r)

    def test_signatures_metadata_status_unknown(self) -> None:
        # The lightweight listing performs no validation: status is "unknown".
        doc = Document(SIGNED_PDF)
        for r in doc.signatures():
            assert r.status == "unknown"
            assert r.reason is None
            assert isinstance(r.field_name, str)

    def test_verify_signatures_matches_validate(self) -> None:
        doc = Document(SIGNED_PDF)
        a = [(r.field_name, r.status) for r in doc.validate_signatures()]
        b = [(r.field_name, r.status) for r in doc.verify_signatures()]
        assert a == b


# ---------------------------------------------------------------------------
# Positive end-to-end: a corpus PDF that surfaces a genuine signature field.
# Proves the validated-result path populates status/field_name/signer — not
# only that an empty list is returned. Skipped when the corpus is absent.
# ---------------------------------------------------------------------------


@pytest.mark.skipif(
    not os.path.exists(SIGNED_CORPUS_PDF),
    reason="signed corpus PDF (regression-388) not checked out",
)
class TestSignedCorpusDocument:
    def test_surfaces_validated_signature(self) -> None:
        doc = Document(SIGNED_CORPUS_PDF)
        results = doc.validate_signatures()
        assert len(results) >= 1
        first = results[0]
        assert isinstance(first, SignatureResult)
        assert first.status in ("valid", "invalid", "unknown")
        assert first.field_name  # non-empty fully-qualified name

    def test_signatures_listing_surfaces_field(self) -> None:
        doc = Document(SIGNED_CORPUS_PDF)
        listing = doc.signatures()
        assert len(listing) >= 1
        # Lightweight listing performs no validation.
        for r in listing:
            assert r.status == "unknown"
            assert r.reason is None
            assert r.field_name


# ---------------------------------------------------------------------------
# Result type / public surface
# ---------------------------------------------------------------------------


class TestSignatureResultType:
    def test_importable_from_package(self) -> None:
        from pdfluent import SignatureResult as SR

        assert SR is SignatureResult

    def test_in_dunder_all(self) -> None:
        assert "SignatureResult" in pdfluent.__all__

    def test_methods_present_on_document(self) -> None:
        doc = Document(SAMPLE_PDF)
        assert hasattr(doc, "validate_signatures")
        assert hasattr(doc, "verify_signatures")
        assert hasattr(doc, "signatures")
        assert callable(doc.validate_signatures)
        assert callable(doc.verify_signatures)
        assert callable(doc.signatures)


# ---------------------------------------------------------------------------
# Signed-payload license functions — importable + callable from the package.
# ---------------------------------------------------------------------------


class TestLicenseFunctionExposure:
    def test_set_license_public_key_exposed(self) -> None:
        assert hasattr(pdfluent, "set_license_public_key")
        assert callable(pdfluent.set_license_public_key)
        assert "set_license_public_key" in pdfluent.__all__

    def test_set_license_payload_exposed(self) -> None:
        assert hasattr(pdfluent, "set_license_payload")
        assert callable(pdfluent.set_license_payload)
        assert "set_license_payload" in pdfluent.__all__

    def test_set_license_public_key_typed_error_on_bad_input(self) -> None:
        # A wrong-length key must raise the typed license exception with a
        # canonical ``.code`` attached — proves the error mapper is wired.
        from pdfluent import PdfluentLicenseError

        with pytest.raises(PdfluentLicenseError) as exc_info:
            pdfluent.set_license_public_key(b"too-short")
        assert exc_info.value.code == "E-LICENSE-INVALID"  # type: ignore[attr-defined]
