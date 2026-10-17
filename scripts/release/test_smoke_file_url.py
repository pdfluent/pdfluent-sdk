#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The ort smoke hands the native CLI a file URL it can read.

On 16-10-2026 the win-x64 ort smoke (release-natives 38847058899, attempt 7)
failed with `could not download file:///c/actions-runner-pdfluent/...: (os
error 3)`: smoke_ort_variant.sh glued "file://" to a Git Bash path, and the
native Windows CLI read `/c/...` as `\\c\\...` on the current drive. The URL
now comes from `smoke_file_url` in smoke_common.sh: `file:///C:/x` on Windows
(cygpath -m, or the MSYS `/c/x` spelling rewritten by hand when cygpath is
missing), `file:///tmp/x` elsewhere.

The helper runs here under bash with a fake `cygpath` and a fake `uname` on
PATH, so the Windows cases run on every CI host.
"""
from __future__ import annotations

import os
import pathlib
import re
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SMOKES = REPO / "scripts" / "release" / "consumer_smokes"
COMMON = SMOKES / "smoke_common.sh"
ORT = SMOKES / "smoke_ort_variant.sh"

# cygpath -m: /c/x -> C:/x, C:\x -> C:/x, C:/x unchanged.
FAKE_CYGPATH = r"""#!/usr/bin/env bash
[[ "$1" == "-m" ]] || { echo "fake cygpath: only -m" >&2; exit 2; }
p="$2"
if [[ "$p" =~ ^/([a-zA-Z])/(.*)$ ]]; then
  d="$(printf '%s' "${BASH_REMATCH[1]}" | tr '[:lower:]' '[:upper:]')"
  p="$d:/${BASH_REMATCH[2]}"
fi
printf '%s\n' "${p//\\//}"
"""


class SmokeFileUrl(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory(prefix="smoke_file_url_")
        root = pathlib.Path(self._tmp.name)
        self.cyg = root / "cyg"
        self.cyg.mkdir()
        (self.cyg / "cygpath").write_text(FAKE_CYGPATH)
        (self.cyg / "cygpath").chmod(0o755)
        self.msys = root / "msys"
        self.msys.mkdir()
        (self.msys / "uname").write_text("#!/bin/sh\necho MINGW64_NT-10.0-26100\n")
        (self.msys / "uname").chmod(0o755)
        # A PATH with bash, tr and printf but no cygpath: the system dirs
        # only, never a dir that could carry a real cygpath.
        self.base_path = "/usr/bin:/bin"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _url(self, path: str, *, extra: pathlib.Path | None = None) -> subprocess.CompletedProcess:
        env = {k: v for k, v in os.environ.items() if k != "PATH"}
        env["PATH"] = f"{extra}{os.pathsep}{self.base_path}" if extra else self.base_path
        return subprocess.run(
            ["bash", "-c", '. "$1"; smoke_file_url "$2"', "bash", str(COMMON), path],
            capture_output=True,
            text=True,
            timeout=60,
            env=env,
        )

    def _ok(self, path: str, want: str, *, extra: pathlib.Path | None = None) -> None:
        proc = self._url(path, extra=extra)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), want, f"{path} with {extra}")

    def test_git_bash_drive_path_with_cygpath(self) -> None:
        self._ok("/c/actions-runner-pdfluent/_work/engine/engine/models-v1",
                 "file:///C:/actions-runner-pdfluent/_work/engine/engine/models-v1",
                 extra=self.cyg)

    def test_git_bash_drive_path_without_cygpath_on_msys(self) -> None:
        self._ok("/e/tmp-ns/models-v1/", "file:///E:/tmp-ns/models-v1", extra=self.msys)

    def test_windows_drive_paths_keep_their_drive(self) -> None:
        for extra in (None, self.cyg):
            self._ok("C:/x", "file:///C:/x", extra=extra)
            self._ok("C:\\x\\models-v1", "file:///C:/x/models-v1", extra=extra)

    def test_posix_paths(self) -> None:
        self._ok("/tmp/x", "file:///tmp/x")
        self._ok("/tmp/x/", "file:///tmp/x")
        # Off MSYS, /c/x is an ordinary POSIX path and stays one.
        self._ok("/c/x", "file:///c/x")

    def test_relative_path_is_refused(self) -> None:
        proc = self._url("models-v1")
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("not an absolute path", proc.stderr)

    def test_the_ort_smoke_builds_its_base_through_the_helper(self) -> None:
        script = ORT.read_text()
        self.assertRegex(script, r'MODELS_URL="\$\(smoke_file_url "\$MODELS_ABS"\)"')
        self.assertIn('export PDFLUENT_MODELS_BASE="$MODELS_URL/"', script)
        glued = re.findall(r'PDFLUENT_MODELS_BASE="file://\$', script)
        self.assertFalse(glued, "file:// glued to a shell path reads as \\c\\... on Windows")


if __name__ == "__main__":
    unittest.main()
