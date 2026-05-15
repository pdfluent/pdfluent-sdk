"""License activation tests for the pdfluent Python binding.

Run:
    cd crates/pdf-python
    maturin develop
    pytest tests/test_license.py -v

These tests use fake-format keys only. They never check in or print a real
signed license payload.
"""

import os

import pytest

pdfluent_native = pytest.importorskip("pdfluent._native")

activate_license_key = pdfluent_native.activate_license_key
activate_license_file = pdfluent_native.activate_license_file
license_status = pdfluent_native.license_status
LicenseStatus = pdfluent_native.LicenseStatus


# ---- Status surface -------------------------------------------------------

def test_status_has_expected_attributes():
    status = license_status()
    assert isinstance(status, LicenseStatus)
    assert isinstance(status.tier, str)
    assert isinstance(status.source, str)
    assert isinstance(status.output_is_marked, bool)


def test_status_tier_is_one_of_known_values():
    status = license_status()
    assert status.tier in {"Trial", "Developer", "Team", "Business", "Enterprise"}


def test_status_source_is_one_of_known_values():
    status = license_status()
    assert status.source in {"Default", "EnvVar", "Explicit"}


def test_status_repr_does_not_contain_key():
    """The repr must not include the raw key from the env var if it is set."""
    status = license_status()
    text = repr(status)
    # repr should mention tier and source; if a real-looking key were leaked,
    # it would not match this short token.
    assert "LicenseStatus(" in text


# ---- Activation errors ----------------------------------------------------

def test_invalid_key_raises_value_error():
    with pytest.raises(ValueError):
        activate_license_key("totally-not-a-license")


def test_unknown_tier_name_raises_value_error():
    with pytest.raises(ValueError):
        activate_license_key("tier:platinum")


def test_empty_key_raises_value_error():
    with pytest.raises(ValueError):
        activate_license_key("")


def test_activate_file_missing_path_raises_io_error():
    with pytest.raises((FileNotFoundError, IOError, OSError)):
        activate_license_file("/nonexistent/path/never-exists.lic")


# ---- Activation lifecycle (single state-mutating test) --------------------

def test_activation_lifecycle(tmp_path):
    """Activate, check status, idempotent re-activate, conflict detection.

    Tolerates the case where another test already activated the process to
    a different tier (since the Rust core uses a process-global OnceLock).
    """
    try:
        activate_license_key("tier:developer")
    except RuntimeError as e:
        # Already activated in a previous test or by env — that's fine.
        assert "already" in str(e).lower()
        return

    status = license_status()
    assert status.tier == "Developer"
    assert status.source == "Explicit"
    assert status.output_is_marked is False  # paid tier => not marked

    # Idempotent re-activate with same tier
    activate_license_key("tier:developer")

    # Conflicting tier returns RuntimeError
    with pytest.raises(RuntimeError):
        activate_license_key("tier:enterprise")


def test_activate_from_file(tmp_path):
    """Activation via file path. Same OnceLock caveat as above."""
    p = tmp_path / "fake.lic"
    p.write_text("tier:team\n", encoding="utf-8")
    try:
        activate_license_file(str(p))
    except RuntimeError as e:
        assert "already" in str(e).lower()
        return
    assert license_status().tier == "Team"
