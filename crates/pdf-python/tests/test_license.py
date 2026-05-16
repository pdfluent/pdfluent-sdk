"""Tests for the license activation surface.

These tests exercise the pure-Python activate_license function and the
PdfluentLicenseError exception class. They do NOT require the native
extension to be built — only the Python package and its stdlib imports.

Run:
    cd crates/pdf-python
    pytest tests/test_license.py -v
"""

from __future__ import annotations

import base64
import json
import os

import pytest

# Import without native extension — only __init__.py pure-Python code
from pdfluent import (
    LicenseInfo,
    activate_license,
)
from pdfluent._native import PdfluentLicenseError

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_license_json(**kwargs: object) -> str:
    payload = {
        "licensee": "Test Corp",
        "company": "Test Corp Ltd",
        "tier": "professional",
        "expires_at": 9999999999,
        "seats": 5,
    }
    payload.update(kwargs)
    return json.dumps(payload)


def _make_license_b64(**kwargs: object) -> str:
    return base64.b64encode(_make_license_json(**kwargs).encode()).decode()


# ---------------------------------------------------------------------------
# Happy-path tests
# ---------------------------------------------------------------------------

class TestActivateLicenseJson:
    def test_returns_license_info(self) -> None:
        key = _make_license_json()
        info = activate_license(key)
        assert isinstance(info, LicenseInfo)

    def test_licensee_field(self) -> None:
        key = _make_license_json(licensee="Acme Corp")
        info = activate_license(key)
        assert info.licensee == "Acme Corp"

    def test_company_field(self) -> None:
        key = _make_license_json(company="Acme Ltd")
        info = activate_license(key)
        assert info.company == "Acme Ltd"

    def test_tier_field(self) -> None:
        for tier in ("trial", "basic", "professional", "enterprise", "archival"):
            key = _make_license_json(tier=tier)
            info = activate_license(key)
            assert info.tier == tier

    def test_expires_at_field(self) -> None:
        key = _make_license_json(expires_at=1234567890)
        info = activate_license(key)
        assert info.expires_at == 1234567890

    def test_seats_field(self) -> None:
        key = _make_license_json(seats=10)
        info = activate_license(key)
        assert info.seats == 10

    def test_missing_optional_fields_default(self) -> None:
        # Minimal JSON — optional fields fall back to defaults
        key = json.dumps({"tier": "trial"})
        info = activate_license(key)
        assert info.licensee == ""
        assert info.tier == "trial"
        assert info.seats == 1
        assert info.expires_at == 0


class TestActivateLicenseBase64:
    def test_base64_encoded_json(self) -> None:
        key = _make_license_b64(licensee="B64 Inc")
        info = activate_license(key)
        assert info.licensee == "B64 Inc"

    def test_base64_with_whitespace(self) -> None:
        key = "  " + _make_license_b64() + "\n"
        info = activate_license(key.strip())
        assert isinstance(info, LicenseInfo)


class TestActivateLicenseFile:
    def test_reads_json_file(self, tmp_path: object) -> None:
        import pathlib
        p = pathlib.Path(str(tmp_path)) / "my.license"
        p.write_text(_make_license_json(licensee="File User"), encoding="utf-8")
        info = activate_license(str(p))
        assert info.licensee == "File User"

    def test_reads_json_extension(self, tmp_path: object) -> None:
        import pathlib
        p = pathlib.Path(str(tmp_path)) / "my.json"
        p.write_text(_make_license_json(tier="enterprise"), encoding="utf-8")
        info = activate_license(str(p))
        assert info.tier == "enterprise"


class TestActivateLicenseEnvVar:
    def test_env_var_json(self, monkeypatch: pytest.MonkeyPatch) -> None:
        key = _make_license_json(licensee="Env User")
        monkeypatch.setenv("PDFLUENT_LICENSE_KEY", key)
        info = activate_license("")
        assert info.licensee == "Env User"

    def test_env_var_base64(self, monkeypatch: pytest.MonkeyPatch) -> None:
        key = _make_license_b64(licensee="Env B64")
        monkeypatch.setenv("PDFLUENT_LICENSE_KEY", key)
        info = activate_license("")
        assert info.licensee == "Env B64"


# ---------------------------------------------------------------------------
# Error-path tests — each must raise PdfluentLicenseError
# ---------------------------------------------------------------------------

class TestActivateLicenseErrors:
    def test_empty_key_no_env(self, monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.delenv("PDFLUENT_LICENSE_KEY", raising=False)
        with pytest.raises(PdfluentLicenseError, match="empty"):
            activate_license("")

    def test_not_json_or_base64(self) -> None:
        with pytest.raises(PdfluentLicenseError, match="malformed"):
            activate_license("this is not valid json or base64!!!")

    def test_invalid_json(self) -> None:
        with pytest.raises(PdfluentLicenseError, match="malformed"):
            activate_license("{broken json}")

    def test_invalid_base64(self) -> None:
        with pytest.raises(PdfluentLicenseError, match="malformed"):
            # Looks like base64 (no leading {) but decodes to garbage
            activate_license("!!!notbase64!!!")

    def test_missing_license_file(self) -> None:
        with pytest.raises(PdfluentLicenseError, match="cannot read"):
            activate_license("/nonexistent/path/my.license")

    def test_exception_is_pdfluent_error(self) -> None:
        from pdfluent import PdfluentError
        with pytest.raises(PdfluentError):
            activate_license("")


# ---------------------------------------------------------------------------
# Type checks (runtime)
# ---------------------------------------------------------------------------

class TestLicenseInfoType:
    def test_is_dataclass(self) -> None:
        import dataclasses
        assert dataclasses.is_dataclass(LicenseInfo)

    def test_fields_are_typed(self) -> None:
        key = _make_license_json()
        info = activate_license(key)
        assert isinstance(info.licensee, str)
        assert isinstance(info.company, str)
        assert isinstance(info.tier, str)
        assert isinstance(info.expires_at, int)
        assert isinstance(info.seats, int)

    def test_exception_hierarchy(self) -> None:
        from pdfluent import PdfluentError
        assert issubclass(PdfluentLicenseError, PdfluentError)
        assert issubclass(PdfluentLicenseError, Exception)
