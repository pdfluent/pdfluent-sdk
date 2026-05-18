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


# ---------------------------------------------------------------------------
# .code attribute — parity with Node, WASM, .NET surfaces
# Closes the documented PYTHON_LICENSE_ERROR_CODE_ATTRIBUTE gap.
# ---------------------------------------------------------------------------

# Canonical C8 codes mirrored from `crates/pdfluent/src/error.rs::Error::code()`.
_CANONICAL_LICENSE_CODES = {
    "E-LICENSE-INVALID",
    "E-LICENSE-FEATURE-NOT-IN-TIER",
    "E-LICENSE-CAPABILITY-NOT-COMPILED",
}


class TestLicenseErrorCode:
    """``e.code`` exposes the canonical C8 error code so callers can branch
    on the failure class without parsing ``str(e)``.

    All raise-sites in the public Python license surface MUST attach a
    canonical code — both the pure-Python pre-validation paths
    (file-not-found, malformed-JSON, unknown tier) and Rust-originated
    errors propagated through ``pdfluent_license_err_to_py``.
    """

    def test_invalid_key_raises_with_code(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.delenv("PDFLUENT_LICENSE_KEY", raising=False)
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license("")
        assert exc_info.value.code == "E-LICENSE-INVALID"  # type: ignore[attr-defined]

    def test_code_is_canonical_c8(self) -> None:
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license("!!!notbase64!!!")
        assert exc_info.value.code in _CANONICAL_LICENSE_CODES  # type: ignore[attr-defined]

    def test_code_set_for_malformed_json(self) -> None:
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license("{broken json}")
        assert exc_info.value.code == "E-LICENSE-INVALID"  # type: ignore[attr-defined]

    def test_code_set_for_missing_file(self) -> None:
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license("/nonexistent/path/my.license")
        assert exc_info.value.code in _CANONICAL_LICENSE_CODES  # type: ignore[attr-defined]

    def test_code_set_for_unparseable_file(self, tmp_path: object) -> None:
        # File exists with the right extension but contains garbage —
        # exercises file-read success then malformed-key parse failure.
        import pathlib
        p = pathlib.Path(str(tmp_path)) / "broken.license"
        p.write_text("not json and not base64!!!", encoding="utf-8")
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license(str(p))
        assert exc_info.value.code == "E-LICENSE-INVALID"  # type: ignore[attr-defined]

    def test_code_set_for_unknown_tier(self) -> None:
        bad_key = json.dumps({"tier": "platinum"})
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license(bad_key)
        assert exc_info.value.code == "E-LICENSE-INVALID"  # type: ignore[attr-defined]

    def test_code_attribute_is_string(self) -> None:
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license("{broken")
        assert isinstance(exc_info.value.code, str)  # type: ignore[attr-defined]
        assert exc_info.value.code.startswith("E-LICENSE-")  # type: ignore[attr-defined]

    def test_message_attribute_present(self) -> None:
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license("{broken")
        assert isinstance(exc_info.value.message, str)  # type: ignore[attr-defined]
        assert "malformed" in exc_info.value.message  # type: ignore[attr-defined]

    def test_code_is_stable_across_call_sites(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: object
    ) -> None:
        """All pure-Python license raise-sites attach ``.code``.

        Today every pre-validation failure maps to ``E-LICENSE-INVALID``
        (the Rust core uses the same code for every ``Error::InvalidLicense``
        variant — different reasons, same canonical code).  This test exists
        so any future regression that drops the ``.code`` attribute on any
        raise-site fails loudly.
        """
        import pathlib

        codes: list[str] = []

        monkeypatch.delenv("PDFLUENT_LICENSE_KEY", raising=False)
        try:
            activate_license("")
        except PdfluentLicenseError as exc:
            codes.append(exc.code)  # type: ignore[attr-defined]

        try:
            activate_license("{not json")
        except PdfluentLicenseError as exc:
            codes.append(exc.code)  # type: ignore[attr-defined]

        try:
            activate_license("!!!neither!!!")
        except PdfluentLicenseError as exc:
            codes.append(exc.code)  # type: ignore[attr-defined]

        try:
            activate_license("/no/such/file.license")
        except PdfluentLicenseError as exc:
            codes.append(exc.code)  # type: ignore[attr-defined]

        p = pathlib.Path(str(tmp_path)) / "junk.license"
        p.write_text("garbage", encoding="utf-8")
        try:
            activate_license(str(p))
        except PdfluentLicenseError as exc:
            codes.append(exc.code)  # type: ignore[attr-defined]

        try:
            activate_license(json.dumps({"tier": "no_such_tier_xyz"}))
        except PdfluentLicenseError as exc:
            codes.append(exc.code)  # type: ignore[attr-defined]

        assert len(codes) == 6
        for c in codes:
            assert c in _CANONICAL_LICENSE_CODES, (
                f"non-canonical license code: {c!r}"
            )

    def test_rust_originated_error_carries_code(self) -> None:
        """Errors that bubble up from the Rust core (e.g. tier-conflict via
        ``set_license_key``) also carry ``.code`` thanks to
        ``pdfluent_license_err_to_py``.

        The autouse fixture has already locked the OnceLock to ``enterprise``,
        so activating a different tier (``trial``) will trigger the Rust core
        to raise ``E-LICENSE-INVALID`` ("license already set").
        """
        with pytest.raises(PdfluentLicenseError) as exc_info:
            activate_license(_make_license_json(tier="trial"))
        # Rust-originated license errors must also carry a code attribute.
        assert hasattr(exc_info.value, "code")
        assert exc_info.value.code in _CANONICAL_LICENSE_CODES  # type: ignore[attr-defined]
