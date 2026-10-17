#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The runner scripts are syntactically sound, shell and PowerShell alike.

shellcheck holds gcp_runner.sh, mac_runner.sh, _vm_common.sh,
selfhosted_setup.sh, the _selfhosted_checks.sh it sources, and
rosetta_x64.sh; the Linux startup script embedded in gcp_runner.sh is
extracted and parsed too, since the outer shellcheck never sees inside the
heredoc. selfhosted_setup.ps1 is parsed by pwsh when pwsh is installed --
otherwise the case skips and says so, and the Windows smoke VM runs it.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]

SHELL_SCRIPTS = [
    "scripts/cos/gcp_runner.sh",
    "scripts/cos/mac_runner.sh",
    "scripts/gcp/_vm_common.sh",
    "scripts/release/selfhosted_setup.sh",
    "scripts/release/_selfhosted_checks.sh",
    "scripts/release/rosetta_x64.sh",
]

PS1 = "scripts/release/selfhosted_setup.ps1"

MINIMUM_CASES = 5  # FLOOR


class RunnerScriptsTests(unittest.TestCase):
    def test_runner_scripts_pass_shellcheck(self):
        shellcheck = shutil.which("shellcheck")
        if shellcheck is None:
            self.skipTest("shellcheck is not installed here")
        cases = 0
        for rel in SHELL_SCRIPTS:
            with self.subTest(rel):
                proc = subprocess.run(
                    [shellcheck, str(REPO / rel)],
                    capture_output=True,
                    text=True,
                    cwd=REPO,
                )
                self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
                cases += 1
        self.assertGreaterEqual(cases, 5)

    def test_embedded_linux_startup_parses(self):
        text = (REPO / "scripts/cos/gcp_runner.sh").read_text()
        # The heredoc may pipe through sed (the pinned-version @TOKENS@);
        # the middle group eats that chain, continuations included.
        match = re.search(
            r"linux_startup\(\) \{\n  cat <<'INNER_EOF'(?:[^\n\\]|\\.)*?\n(.*?)INNER_EOF",
            text,
            re.S,
        )
        self.assertIsNotNone(match, "linux_startup holds no INNER_EOF heredoc")
        with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False) as handle:
            handle.write(match.group(1))
            inner = handle.name
        try:
            proc = subprocess.run(["bash", "-n", inner], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
        finally:
            pathlib.Path(inner).unlink()

    def test_windows_setup_parses(self):
        pwsh = shutil.which("pwsh")
        if pwsh is None:
            self.skipTest(
                "pwsh is not installed here; the Windows smoke VM runs this file"
            )
        script = (
            "$errs = $null; "
            "[void][System.Management.Automation.Language.Parser]::ParseFile("
            f"'{REPO / PS1}', [ref]$null, [ref]$errs); "
            "if ($errs.Count -gt 0) { $errs | ForEach-Object { $_.Message }; exit 1 }"
        )
        proc = subprocess.run(
            [pwsh, "-NoProfile", "-Command", script], capture_output=True, text=True
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)


if __name__ == "__main__":
    unittest.main()
