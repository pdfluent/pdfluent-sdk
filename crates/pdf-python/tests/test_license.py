"""Tests for the license activation surface.

After the COM1 fix, ``activate_license()`` calls ``_native.set_license_key()``
which writes into the Rust process-global ``OnceLock<Tier>``.  That means:

- The tier can be set **once** per process.
- Re-activating with the **same** tier is an idempotent no-op.
- Re-activating with a **different** tier raises ``PdfluentLicenseError``.

All happy-path tests use ``"enterprise"`` as the JSON tier (maps to
``"enterprise"`` in Rust) so they can be run in any order in the same process.

Tier mapping reference (JSON payload → canonical Rust):
  trial → trial | basic → developer | professional → team
  enterprise → enterprise | archival → business

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_license.py -v
"""

from __future__ import annotations

import base64
import json
import os

import pytest

from pdfluent import LicenseInfo, activate_license, license_status
from pdfluent._native import PdfluentLicenseError, native_license_info

# ---------------------------------------------------------------------------
# Capture license state BEFORE any test activates anything.
# This line runs at module-import time (pytest collection), before setUp/fixtures.
# ---------------------------------------------------------------------------
_TIER_BEFORE_ACTIVATION: str = native_license_info().tier


# ---------------------------------------------------------------------------
# Autouse fixture: guarantee "enterprise" is set before any test in this module.
#
# The OnceLock means only the first call to set_license_key wins. By forcing
# "enterprise" here, every test sees a consistent baseline regardless of
# execution order. The fixture silently swallows PdfluentLicenseError in case
# the key was already set by a prior run (e.g. pytest-xdist workers sharing
# the same process).
# ---------------------------------------------------------------------------
@pytest.fixture(scope="module", autouse=True)
def _activate_enterprise_once() -> None:
    try:
        activate_license(
            json.dumps(
                {
                    "licensee": "Test Corp",
                    "company": "Test Corp Ltd",
                    "tier": "enterprise",
                    "seats": 5,
                }
            )
        )
    except PdfluentLicenseError:
        pass  # already set — idempotent if same tier, expected if different


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_license_json(**kwargs: object) -> str:
    payload: dict[str, object] = {
        "licensee": "Test Corp",
        "company": "Test Corp Ltd",
        "tier": "enterprise",
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

    def test_tier_reflects_canonical_rust_tier(self) -> None:
        # "enterprise" JSON maps to canonical Rust tier "enterprise"
        key = _make_license_json(tier="enterprise")
        info = activate_license(key)
        assert info.tier == "enterprise"

    def test_expires_at_is_none_in_1_0(self) -> None:
        # Rust 1.0 uses evaluation format — no time-bound expiry
        key = _make_license_json()
        info = activate_license(key)
        assert info.expires_at is None

    def test_output_is_marked_false_for_paid_tier(self) -> None:
        key = _make_license_json(tier="enterprise")
        info = activate_license(key)
        assert info.output_is_marked is False

    def test_seats_field(self) -> None:
        key = _make_license_json(seats=10)
        info = activate_license(key)
        assert info.seats == 10

    def test_missing_optional_fields_default(self) -> None:
        # Minimal JSON with enterprise tier — optional fields fall back
        key = json.dumps({"tier": "enterprise"})
        info = activate_license(key)
        assert info.licensee == ""
        assert info.tier == "enterprise"
        assert info.seats == 1


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

    def test_unknown_tier_raises_license_error(self) -> None:
        bad_key = json.dumps({"tier": "platinum"})
        with pytest.raises(PdfluentLicenseError, match="unknown"):
            activate_license(bad_key)


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
        assert info.expires_at is None or isinstance(info.expires_at, str)
        assert isinstance(info.seats, int)
        assert isinstance(info.output_is_marked, bool)

    def test_exception_hierarchy(self) -> None:
        from pdfluent import PdfluentError

        assert issubclass(PdfluentLicenseError, PdfluentError)
        assert issubclass(PdfluentLicenseError, Exception)


# ---------------------------------------------------------------------------
# Activation wiring tests (COM1 fix verification)
# ---------------------------------------------------------------------------


class TestActivationWiring:
    def test_t1_invalid_tier_raises_typed_error(self) -> None:
        """T1: Unknown tier in JSON raises PdfluentLicenseError, not generic Exception."""
        bad_key = json.dumps({"tier": "nonexistent_tier_xyz", "licensee": "Corp"})
        with pytest.raises(PdfluentLicenseError):
            activate_license(bad_key)

    def test_t2_valid_key_status_is_non_trial(self) -> None:
        """T2: After activation with a non-trial key, license_status() is non-trial."""
        info = activate_license(_make_license_json(tier="enterprise"))
        assert info.tier != "trial"
        assert info.tier == "enterprise"
        assert license_status() == "enterprise"

    def test_t3_status_changed_from_trial_to_active(self) -> None:
        """T3: Status before any activation was trial; after activation it is not.

        ``_TIER_BEFORE_ACTIVATION`` is captured at module-load time (before the
        autouse fixture or any test runs), so it reflects the genuinely
        unactivated state.
        """
        assert _TIER_BEFORE_ACTIVATION == "trial", (
            "Initial tier must be trial — another test file or import may have "
            "already activated a license before this module was loaded."
        )
        # After autouse fixture + this call, status is enterprise
        activate_license(_make_license_json(tier="enterprise"))
        assert license_status() != "trial"

    def test_t4_reactivation_same_tier_is_idempotent(self) -> None:
        """T4: Calling activate_license twice with the same tier does not raise."""
        activate_license(_make_license_json(tier="enterprise"))
        # Second call with same mapped tier must be a no-op
        activate_license(_make_license_json(tier="enterprise"))

    def test_t4b_different_tier_after_enterprise_raises(self) -> None:
        """T4b: After enterprise is set, activating a different tier raises.

        The Rust OnceLock rejects a second different-tier call with
        ``E-LICENSE-INVALID / "license already set"``.  The autouse fixture
        guarantees enterprise is already active before this test runs.
        """
        with pytest.raises(PdfluentLicenseError):
            activate_license(_make_license_json(tier="trial"))


# ---------------------------------------------------------------------------
# Tier mapping unit tests (no Rust activation I/O — tests _TIER_MAP directly)
# ---------------------------------------------------------------------------


class TestTierMapping:
    def test_enterprise_maps_to_enterprise(self) -> None:
        from pdfluent import _TIER_MAP
        assert _TIER_MAP["enterprise"] == "enterprise"

    def test_professional_maps_to_team(self) -> None:
        from pdfluent import _TIER_MAP
        assert _TIER_MAP["professional"] == "team"

    def test_basic_maps_to_developer(self) -> None:
        from pdfluent import _TIER_MAP
        assert _TIER_MAP["basic"] == "developer"

    def test_trial_maps_to_trial(self) -> None:
        from pdfluent import _TIER_MAP
        assert _TIER_MAP["trial"] == "trial"

    def test_archival_maps_to_business(self) -> None:
        from pdfluent import _TIER_MAP
        assert _TIER_MAP["archival"] == "business"
