#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The Windows self-hosted setup works on a bare windows-2022 VM (#636).

Static checks over scripts/release/selfhosted_setup.ps1, runnable on
Linux/macOS CI: the public windows-2022 image has no package manager, no
Python and no Git Bash, so the script downloads python.org's installer
(sha256-pinned per minor), applies the machine settings the build needs,
refuses without bash, and installs the zip tools machine-wide.

SELFHOSTED_PS1 points the suite at another copy (the red proof runs it
against the m308 draft); default is the tree's script.
"""
from __future__ import annotations

import os
import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
PS1 = pathlib.Path(os.environ.get("SELFHOSTED_PS1", REPO / "scripts" / "release" / "selfhosted_setup.ps1"))

# Minor -> (patch, sha256 of python.org's amd64 exe). The hashes are pinned
# here a second time on purpose: an edit that touches the table must touch
# this file too, naming the new bytes it vouches for.
PINS = {
    "3.11": ("3.11.9", "5ee42c4eee1e6b4464bb23722f90b45303f79442df63083f05322f1785f5fdde"),
    "3.12": ("3.12.10", "67b5635e80ea51072b87941312d00ec8927c4db9ba18938f7ad2d27b328b95fb"),
}

MINIMUM_CASES = 11  # FLOOR


def body(text: str, name: str) -> str:
    """The function body of `function <name>`, up to the next top-level function."""
    start = text.index(f"function {name} ")
    rest = text[start:]
    m = re.search(r"\nfunction \w", rest[1:])
    return rest[: m.start() + 1] if m else rest


class SelfhostedSetupPs1Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = PS1.read_text()

    def test_python_table_holds_a_sha256_per_pinned_patch(self):
        rows = re.findall(
            r'"(3\.\d+)"\s*=\s*@\{\s*Patch\s*=\s*"([^"]+)";\s*Sha256\s*=\s*"([^"]+)"',
            self.text,
        )
        self.assertGreaterEqual(len(rows), 2, "the python pin table holds fewer than two minors")
        found = {minor: (patch, sha) for minor, patch, sha in rows}
        for minor, (patch, sha) in PINS.items():
            with self.subTest(minor=minor):
                self.assertIn(minor, found, f"no pin row for {minor}")
                self.assertEqual(found[minor][0], patch)
                self.assertEqual(found[minor][1], sha)
                self.assertRegex(sha, r"^[0-9a-f]{64}$")
        # And the installer URL is built from the pinned patch, amd64 only.
        self.assertIn("python-$patch-amd64.exe", self.text)

    def test_python_install_is_machine_wide_and_hash_checked(self):
        setup = body(self.text, "Setup-Python")
        for flag in ("InstallAllUsers=1", "PrependPath=0", "Include_test=0", "TargetDir="):
            self.assertIn(flag, setup, f"machine-wide quiet install needs {flag}")
        self.assertIn("Get-FileHash", setup)
        self.assertIn("SHA256", setup)
        self.assertRegex(setup, r"(?i)sha256 mismatch")
        self.assertIn("Die", setup)

    def test_no_package_manager_path_remains(self):
        offenders = [n for n, line in enumerate(self.text.splitlines(), 1) if "winget" in line.lower()]
        self.assertEqual(offenders, [], f"winget named on line(s) {offenders}: the bare image has none")

    def test_machine_settings_present_and_elevated(self):
        self.assertIn("function Set-MachineSettings ", self.text)
        self.assertIn("function Test-IsElevated ", self.text)
        setup = body(self.text, "Set-MachineSettings")
        for needle in ("LongPathsEnabled", "core.longpaths", "Add-MpPreference", "ExclusionPath", ".cargo", ".rustup"):
            self.assertIn(needle, setup, f"machine settings must cover {needle}")
        # Guarded by an elevation check: not elevated + something to change refuses.
        self.assertIn("Test-IsElevated", setup)
        self.assertIn("Die", setup)
        elevated = body(self.text, "Test-IsElevated")
        self.assertIn("Administrator", elevated)
        # The runner root comes from the workspace's first two components.
        self.assertIn("GITHUB_WORKSPACE", setup)
        self.assertIn("C:\\actions-runner", setup)
        # The Defender part is skipped, loudly, when the cmdlet is absent.
        self.assertIn("Get-MpPreference", setup)

    def test_bash_refusal_names_wsl(self):
        self.assertIn("function Test-BashReady ", self.text)
        check = body(self.text, "Test-BashReady")
        for needle in ("bash.exe", "System32", "WSL", "MinGit", "Git for Windows", "Die"):
            self.assertIn(needle, check, f"the bash check must name {needle}")
        self.assertIn("bash --version", check)

    def test_zip_tools_live_under_the_machine_wide_root(self):
        self.assertIn('$ToolsRoot = "C:\\pdfluent-tools"', self.text)
        for tool in ("Setup-Node", "Setup-Java"):
            with self.subTest(tool=tool):
                self.assertIn("ToolsRoot", body(self.text, tool))
        offenders = [
            n
            for n, line in enumerate(self.text.splitlines(), 1)
            if 'USERPROFILE "tools' in line or "USERPROFILE 'tools" in line
        ]
        self.assertEqual(offenders, [], f"per-user tools dir on line(s) {offenders}")

    def test_vswhere_asks_for_a_real_component_id(self):
        # vswhere matches -requires against exact IDs: the bare
        # "...Component.VC.Tools" names no component, so it found nothing on a
        # VM with the Build Tools installed (smoke run 37305676691).
        reqs = re.findall(r"-requires\s+(\S+)", self.text)
        self.assertGreaterEqual(len(reqs), 1)
        for req in reqs:
            self.assertEqual(req, "Microsoft.VisualStudio.Component.VC.Tools.x86.x64")

    def test_python_installer_is_waited_for(self):
        # A GUI-subsystem installer run with & returns before it finishes;
        # run 37310783497 then failed deleting the still-open exe.
        self.assertNotIn("& $tmp /quiet", self.text)
        self.assertRegex(self.text, r"Start-Process -FilePath \$tmp -ArgumentList \"/quiet\"[^\n]*-Wait -PassThru")
        self.assertIn("$proc.ExitCode -notin 0, 3010", self.text)

    def test_napi_version_comes_from_npm_and_the_script_ends_in_success(self):
        # Run 37331673176: `napi --version` is a usage error on the 2.x CLI,
        # and the runner's pwsh wrapper turned its exit 1 into the step's.
        self.assertNotIn("napi --version", self.text)
        self.assertIn('npm ls -g --depth=0 --json "@napi-rs/cli"', self.text)
        self.assertTrue(self.text.rstrip().endswith("exit 0"), "the script must end with exit 0")

    def test_cargo_links_with_msvc_not_git_coreutils(self):
        # Run 37335333934: bash's PATH found C:\\Git\\usr\\bin\\link.exe first.
        self.assertIn('Set-StepEnv "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER" $MsvcLink', self.text)
        self.assertIn('Join-Path $env:VCToolsInstallDir "bin\\HostX64\\x64\\link.exe"', self.text)

    def test_node_takes_a_major_or_an_exact_version(self):
        setup = body(self.text, "Setup-Node")
        self.assertIn("20.18.1", setup)
        self.assertRegex(setup, r"(?i)resolves to \$ver")

    def test_zig_probe_goes_through_the_module_not_a_zig_on_path(self):
        # Build 2, win-x64: the ziglang wheel ships no `zig` console
        # script (its only entry point is `python-zig`), so the version
        # log's `& zig version` aborted setup with "The term 'zig' is not
        # recognized". Both the keep-check and the version log probe
        # `python -m ziglang version`, which runs on every pool.
        self.assertIn("function Get-ZigVersion ", self.text)
        probe = body(self.text, "Get-ZigVersion")
        self.assertIn("python -m ziglang version", probe)
        setup = body(self.text, "Setup-Zig")
        self.assertIn("Get-ZigVersion", setup)
        code = "\n".join(
            line.split("#", 1)[0] for line in self.text.splitlines()
        )
        self.assertNotIn("Get-Command zig", code)
        self.assertNotIn("zig version", code.replace("ziglang version", ""))

    def test_python_setup_leaves_a_python3_shim_on_path(self):
        # Build 3, win-x64: the python.org installer ships python.exe only,
        # and the workflows' bash steps call python3 (`python3: command not
        # found` in Package binary and Build the wheel). The setup leaves a
        # python3.exe beside python.exe — a dir it already puts on PATH, so
        # Git Bash resolves it — on both the kept and the fresh path, and
        # logs it.
        self.assertIn("function Set-Python3Shim ", self.text)
        shim = body(self.text, "Set-Python3Shim")
        self.assertIn("python3.exe", shim)
        self.assertIn("Copy-Item", shim)
        self.assertIn("Log", shim)
        setup = body(self.text, "Setup-Python")
        calls = re.findall(r"Set-Python3Shim \$targetDir", setup)
        self.assertEqual(len(calls), 2, "the kept and the fresh path both shim")

    def test_zig_version_log_never_fails_the_setup(self):
        # The log line is what aborted build 2: a probe that cannot
        # answer reports "unknown" instead of throwing under Stop.
        probe = body(self.text, "Get-ZigVersion")
        self.assertIn("try", probe)
        self.assertIn("catch", probe)
        setup = body(self.text, "Setup-Zig")
        self.assertIn('"unknown"', probe + setup)
        self.assertIn("Get-CargoZigbuildVersion", setup)

    def test_the_opt_in_keeps_todays_defender_behaviour(self):
        # With PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS=1 (the ephemeral
        # VMs) the setup keeps today's behaviour: missing exclusions are
        # collected, required and added.
        setup = body(self.text, "Set-MachineSettings")
        self.assertIn('$defenderOptIn = ($env:PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS -eq "1")', setup)
        gated = re.search(r"if \(\$defenderOptIn -and \$mp\) \{(.*?)\n    \}", setup, re.S)
        self.assertIsNotNone(gated, "the exclusion collection sits behind the opt-in guard")
        assert gated is not None
        self.assertIn("ExclusionPath", gated.group(1))
        self.assertIn("$missingExcl += $e", gated.group(1))
        self.assertIn("Add-MpPreference -ExclusionPath $missingExcl", setup)
        self.assertIn('$missing += "Defender exclusion $e"', setup)

    def test_without_the_opt_in_defender_warns_and_continues(self):
        # Without the opt-in (the persistent desktop runner) the setup
        # never touches Defender: when real-time protection is on it logs
        # one WARNING and continues with exit 0, and no Defender entry
        # can reach the refusal.
        setup = body(self.text, "Set-MachineSettings")
        self.assertIn("DisableRealtimeMonitoring", setup)
        self.assertRegex(setup, r"if \(-not \$defenderOptIn -and \$mp\)")
        self.assertRegex(
            setup,
            r'Log "WARNING: Defender real-time protection is on for '
            r'.*;\s*builds may be slower; '
            r"set PDFLUENT_SELFHOSTED_DEFENDER_EXCLUSIONS=1 on ephemeral VMs only\"",
        )
        self.assertLess(
            setup.index("} elseif (-not $defenderOptIn) {"),
            setup.index("Add-MpPreference"),
            "the add-exclusions branch is unreachable without the opt-in",
        )
        self.assertEqual(setup.count("Add-MpPreference"), 1)
        self.assertIn("machine settings: already set (LongPathsEnabled=1, git core.longpaths)", setup)

    def test_long_paths_stay_required_without_the_opt_in(self):
        # Long paths and git longpaths stay hard requirements with the
        # same refusal, whether or not the opt-in is set.
        setup = body(self.text, "Set-MachineSettings")
        self.assertIn('$missing += "LongPathsEnabled=1"', setup)
        self.assertIn('$missing += "git core.longpaths=true"', setup)
        self.assertIn("Die \"machine settings need an elevated shell", setup)
        self.assertIn("$($missing -join '; ')", setup)
        self.assertLess(
            setup.index("$needLp = "),
            setup.index("$defenderOptIn = "),
            "the long-paths requirement is computed before the opt-in",
        )


if __name__ == "__main__":
    unittest.main()
