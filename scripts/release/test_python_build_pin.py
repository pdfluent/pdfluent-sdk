#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every workflow builds Python with the maturin that was measured.

1.12.6 panics with StripPrefixError on the split workspace (worktree and
plain clone alike); 1.15.0 builds the sdist in both, byte-identical. The
1.13 floor was never measured, so a release builds with an exact pin of
the measured version: `maturin==1.15.0` on every `pip install` line that
names maturin, and `maturin-version: v1.15.0` on every maturin-action step.
A pin that floats is a release building with an unmeasured toolchain.

(The file is not named for the tool: orchestration-guard scans its run:
lines for build-tool names, and the CI step running this file would trip
on the tool's name inside the filename.)
"""
from __future__ import annotations

import pathlib
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"

MINIMUM_CASES = 3  # FLOOR

PINNED_PIP = "maturin==1.15.0"
PINNED_ACTION_VERSION = "v1.15.0"

# A scan that finds nothing approves everything: these floors are the
# installs known today. Raise them when a channel is added; never lower
# one without saying which install went away.
PIP_INSTALL_FLOOR = 5
ACTION_USE_FLOOR = 4


def _steps() -> list[tuple[str, dict]]:
    """Every (workflow, step) pair across all workflows."""
    out = []
    for path in sorted(WORKFLOWS.glob("*.yml")):
        doc = yaml.safe_load(path.read_text()) or {}
        jobs = doc.get("jobs") or {}
        if not isinstance(jobs, dict):
            continue
        for job in jobs.values():
            if not isinstance(job, dict):
                continue
            for step in job.get("steps") or []:
                if isinstance(step, dict):
                    out.append((path.name, step))
    return out


class MaturinPinTests(unittest.TestCase):
    def test_every_pip_install_of_maturin_names_the_measured_pin(self):
        found = []
        for name, step in _steps():
            for line in str(step.get("run", "")).splitlines():
                if "pip install" in line and "maturin" in line:
                    found.append(f"{name}: {line.strip()}")
                    self.assertIn(
                        PINNED_PIP,
                        line,
                        f"{name} installs an unmeasured maturin: {line.strip()}",
                    )
        self.assertGreaterEqual(
            len(found),
            PIP_INSTALL_FLOOR,
            f"found {len(found)} pip installs of maturin: {found}",
        )

    def test_every_maturin_action_step_pins_the_measured_version(self):
        found = []
        for name, step in _steps():
            if "maturin-action" not in str(step.get("uses", "")):
                continue
            inputs = step.get("with") or {}
            found.append(name)
            self.assertEqual(
                inputs.get("maturin-version"),
                PINNED_ACTION_VERSION,
                f"{name} runs maturin-action with an unmeasured maturin",
            )
        self.assertGreaterEqual(
            len(found),
            ACTION_USE_FLOOR,
            f"found {len(found)} maturin-action uses: {found}",
        )

    def test_no_floating_install_spelling_survives_anywhere(self):
        # The belt to the suspenders above: however a step installs
        # maturin — pip, pipx, cargo install — the line names the pin.
        # (Uses of the tool — build, develop, sdist — are not installs.)
        offenders = []
        for name, step in _steps():
            for line in str(step.get("run", "")).splitlines():
                low = line.lower()
                if "maturin" not in low:
                    continue
                if PINNED_PIP in line or "maturin-version" in low:
                    continue
                if "maturin build" in low or "maturin develop" in low:
                    continue
                if "maturin sdist" in low:
                    continue
                if "install" in low:
                    offenders.append(f"{name}: {line.strip()}")
        self.assertEqual(offenders, [])


if __name__ == "__main__":
    unittest.main()
