#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The .NET smoke restores into a packages dir of its own.

NuGet's global packages folder (~/.nuget/packages) is keyed by package id and
version. A smoke that restores into it reuses a PDFluent.<version> extracted
by an earlier run on the same host, and the nupkg under test is never opened:
a rebuilt nupkg with the same version passes on the old bytes. The smoke must
therefore restore with `--packages <its own temp dir>/packages` and build with
`--no-restore`, so no implicit restore goes back to the global folder.

The cases run the real smoke_dotnet.sh against a fake `dotnet` on PATH that
records every call, so they need no .NET SDK and run on every CI host.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SMOKE = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_dotnet.sh"

# Records "<cwd>\t<argv...>" per call; `restore` fails when FAKE_DOTNET_FAIL
# names it, `run` prints the pass line the real program prints.
FAKE_DOTNET = """#!/usr/bin/env bash
printf '%s' "$PWD" >> "$FAKE_DOTNET_LOG"
for a in "$@"; do printf '\\t%s' "$a" >> "$FAKE_DOTNET_LOG"; done
printf '\\n' >> "$FAKE_DOTNET_LOG"
if [[ "${1:-}" == "${FAKE_DOTNET_FAIL:-none}" ]]; then echo "fake $1 failure" >&2; exit 1; fi
if [[ "${1:-}" == "run" ]]; then echo "PDFluent .NET smoke PASS"; fi
exit 0
"""


class DotnetSmokeRestoresIntoItsOwnPackagesDir(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory(prefix="dotnet_pkgs_test_")
        self.root = pathlib.Path(self._tmp.name).resolve()
        bindir = self.root / "bin"
        bindir.mkdir()
        fake = bindir / "dotnet"
        fake.write_text(FAKE_DOTNET)
        fake.chmod(0o755)
        self.nupkg = self.root / "PDFluent.9.9.9.nupkg"
        self.nupkg.write_bytes(b"not a real nupkg; the fake dotnet never opens it")
        self.smoke_tmp = self.root / "tmp"
        self.smoke_tmp.mkdir()
        self.bindir = bindir

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, log: pathlib.Path, fail: str | None = None) -> subprocess.CompletedProcess:
        env = dict(os.environ)
        env["PATH"] = f"{self.bindir}{os.pathsep}{env.get('PATH', '')}"
        env["FAKE_DOTNET_LOG"] = str(log)
        env["TMPDIR"] = str(self.smoke_tmp)
        env.pop("PDFLUENT_RELEASE_LANE", None)
        env.pop("NUGET_PACKAGES", None)
        if fail:
            env["FAKE_DOTNET_FAIL"] = fail
        return subprocess.run(
            ["bash", str(SMOKE), "--artifact", str(self.nupkg), "--version", "9.9.9"],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=REPO,
            env=env,
        )

    @staticmethod
    def _calls(log: pathlib.Path) -> list[tuple[str, list[str]]]:
        calls = []
        for line in log.read_text().splitlines():
            cwd, *argv = line.split("\t")
            calls.append((cwd, argv))
        return calls

    def _packages_arg(self, argv: list[str]) -> str:
        self.assertIn("--packages", argv, f"restore without --packages: {argv}")
        return argv[argv.index("--packages") + 1]

    def test_restore_names_a_packages_dir_inside_the_smoke_dir(self) -> None:
        log = self.root / "calls.log"
        proc = self._run(log)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        calls = self._calls(log)
        verbs = [argv[0] for _, argv in calls]
        self.assertIn("restore", verbs, f"no explicit restore; calls: {calls}")
        cwd, argv = calls[verbs.index("restore")]
        packages = pathlib.Path(self._packages_arg(argv)).resolve()
        smoke_dir = pathlib.Path(cwd).resolve().parent
        self.assertEqual(packages, smoke_dir / "packages")
        self.assertIn(
            "pdfluent_dotnet_smoke_", smoke_dir.name,
            f"the restore ran outside the smoke's own temp dir: {cwd}",
        )
        home_cache = pathlib.Path.home() / ".nuget" / "packages"
        self.assertNotEqual(packages, home_cache)

    def test_build_does_not_restore_again(self) -> None:
        log = self.root / "calls.log"
        proc = self._run(log)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        verbs = [argv for _, argv in self._calls(log)]
        builds = [argv for argv in verbs if argv[0] == "build"]
        self.assertEqual(len(builds), 1, f"calls: {verbs}")
        self.assertIn("--no-restore", builds[0])
        names = [argv[0] for argv in verbs]
        self.assertLess(names.index("restore"), names.index("build"))

    def test_each_run_gets_a_fresh_packages_dir(self) -> None:
        dirs = []
        for n in (1, 2):
            log = self.root / f"calls{n}.log"
            proc = self._run(log)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            restore = next(argv for _, argv in self._calls(log) if argv[0] == "restore")
            dirs.append(self._packages_arg(restore))
        self.assertNotEqual(dirs[0], dirs[1])

    def test_a_failed_restore_fails_the_smoke_naming_it(self) -> None:
        log = self.root / "calls.log"
        proc = self._run(log, fail="restore")
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("FAIL (restore)", proc.stdout + proc.stderr)
        verbs = [argv[0] for _, argv in self._calls(log)]
        self.assertNotIn("build", verbs)
        self.assertNotIn("run", verbs)


if __name__ == "__main__":
    unittest.main()
