#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The self-hosted setup keeps tools that are already present (#636).

Every keep-check runs under `set -euo pipefail`, so a `... | grep -q ...`
check is not idempotent: grep exits on the first match, the producer dies
of SIGPIPE, and pipefail turns the match into "not present" -- the setup
then reinstalls on every run. The checks capture the version into a
variable and compare the string instead.

Each case sources the real keep-check functions from
_scripts/release/_selfhosted_checks.sh_ (the file selfhosted_setup.sh
sources) and calls one setup function, with stubbed tools whose version
output is a megabyte long, so the SIGPIPE that the `grep -q` form needs
is certain rather than racy. A kept tool logs "(already present)" and
never reaches its installer; a tool at the wrong version still
reinstalls. The cases name only the checks file: the main script holds
install lines no push-lane gate may reach.

The napi probe is the same shape one level down. @napi-rs/cli 2.x has no
--version flag ("Unsupported option name"), so the keep-check cannot ask
the binary: napi_installed_version reads the globally installed version
from `npm ls -g`, parsed with node (no jq on the runners), printing the
bare version or nothing. The cases stub npm and node; the red-proof runs
the old `napi --version` form against the installed-at-the-pin fixture
and watches it miss.
"""
from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SETUP = REPO / "scripts/release/selfhosted_setup.sh"
CHECKS = REPO / "scripts/release/_selfhosted_checks.sh"
# Absolute: an isolated PATH hides `bash` from the child env lookup.
BASH = shutil.which("bash") or "bash"

# The driver sources the keep-check functions and calls one setup function.
# The checks path rides as $0, so the call names the file it runs and the
# push-compile guard can see the edge.
DRIVER = 'set -euo pipefail; . "$0"; "setup_$1"'

# Past any pipe buffer (64 KiB), so a `grep -q` consumer has exited long
# before the stub is done writing: the red form fails deterministically.
PADDING = "yes 'padding padding padding padding' | head -c 1000000"

# The napi driver sources the keep-check functions and compares the probe
# to the pin it is given -- the keep-check setup_napi runs, without the
# install path the push lane may not reach.
NAPI_DRIVER = (
    'set -euo pipefail; . "$0"; '
    'have="$(napi_installed_version)"; '
    'if [ "$have" = "$1" ]; then echo present; else echo "not present"; fi'
)

# The probe setup_napi ran before: `napi --version`, which @napi-rs/cli
# 2.x answers with "Unsupported option name". The red-proof runs this
# form against the installed-at-the-pin fixture and watches it miss.
OLD_NAPI_DRIVER = (
    'set -euo pipefail; . "$0"; '
    'if command -v napi >/dev/null 2>&1 && [ "$(napi --version 2>/dev/null)" = "$1" ]; '
    'then echo present; else echo "not present"; fi'
)

NAPI_PIN = "2.18.0"
NAPI_OTHER = "2.17.0"


def _run(driver: str, arg: str, stubs: dict[str, str],
         versions: dict[str, str],
         isolate_path: bool = False) -> tuple[int, str, str]:
    """Source the keep-check functions and run one driver; (exit, out, stub log).

    With isolate_path the driver sees only the stubs: `command -v cc`
    consults the real PATH, which carries a cc on every dev and CI
    machine, so the cc-missing cases hide it by confining PATH to bindir
    (and stubbing the passthrough tools the driver still reaches)."""
    with tempfile.TemporaryDirectory(prefix="keep-checks-") as raw:
        tmp = pathlib.Path(raw)
        bindir = tmp / "bin"
        bindir.mkdir()
        home = tmp / "home"
        home.mkdir()
        for name, body in stubs.items():
            stub = bindir / name
            stub.write_text("#!/bin/sh\n" + body)
            stub.chmod(0o755)
        log = tmp / "stub.log"
        log.write_text("")
        path = str(bindir) + os.pathsep + os.environ["PATH"]
        env = dict(os.environ, PATH=path,
                   HOME=str(home), STUB_LOG=str(log), **versions)
        if isolate_path:
            env["PATH"] = str(bindir)
        env.pop("GITHUB_PATH", None)
        env.pop("GITHUB_ENV", None)
        proc = subprocess.run(
            [BASH, "-c", driver, str(CHECKS), arg],
            capture_output=True, text=True, timeout=120, env=env, cwd=tmp,
        )
        return proc.returncode, proc.stdout + proc.stderr, log.read_text()


def run_check(tool: str, stubs: dict[str, str],
              versions: dict[str, str],
              isolate_path: bool = False) -> tuple[int, str, str]:
    """Source the keep-check functions and run one; (exit, out, stub log)."""
    return _run(DRIVER, tool, stubs, versions, isolate_path)


def run_probe(stubs: dict[str, str], pin: str) -> tuple[int, str, str]:
    """Source the keep-check functions and compare the napi probe to a pin."""
    return _run(NAPI_DRIVER, pin, stubs, {})


def npm_reporting(body: str) -> str:
    """The npm stub, answering `ls` with the given body."""
    return NPM_STUB.replace("NPM_BODY", body)


def npm_json(version: str) -> str:
    """The `npm ls -g --json` shape with @napi-rs/cli at one version."""
    return '{"dependencies":{"@napi-rs/cli":{"version":"%s"}}}' % version


PIP3_STUB = """\
if [ "$1" = "show" ]; then
  echo "Name: maturin"
  echo "VERSION_LINE"
  PADDING
elif [ "$1" = "install" ]; then
  echo "INSTALL $*" >> "$STUB_LOG"
  exit 0
fi
"""

RUSTUP_STUB = """\
if [ "$1" = "toolchain" ] && [ "$2" = "list" ]; then
  echo "TOOLCHAIN_LINE"
  PADDING
elif [ "$1" = "toolchain" ] && [ "$2" = "install" ]; then
  echo "INSTALL $*" >> "$STUB_LOG"
  exit 0
fi
# No trailing `exit 0`: the list branch must report a SIGPIPE death, which
# is the failure the old `grep -q` form produces under pipefail.
"""

DOTNET_STUB = """\
if [ "$1" = "--list-sdks" ]; then
  echo "SDK_LINE"
  PADDING
elif [ "$1" = "--version" ]; then
  echo "8.0.100"
fi
"""

NPM_STUB = """\
if [ "$1" = "ls" ]; then
  echo "ARGS $*" >> "$STUB_LOG"
  NPM_BODY
else
  echo "UNEXPECTED $*" >> "$STUB_LOG"
  exit 9
fi
"""

# Emulates the one `node -e <script> <json>` call the probe makes: the
# script reads the JSON from its first argument ($3 here) and prints
# dependencies.@napi-rs/cli.version, or nothing when it is absent.
NODE_STUB = """\
json="$3"
printf '%s' "$json" | python3 -c 'import json, sys; print((json.load(sys.stdin).get("dependencies") or {}).get("@napi-rs/cli", {}).get("version") or "")'
"""

# `napi` as @napi-rs/cli 2.x ships it: --version is not a flag.
NAPI2_STUB = """\
echo 'napi: Unsupported option name' >&2
exit 1
"""

# The apt-get stub, recording every call it receives.
APT_STUB = """\
echo "APT $*" >> "$STUB_LOG"
exit 0
"""

# cut under an isolated PATH: the real one, reached absolutely.
CUT_STUB = """\
PATH=/usr/bin:/bin exec cut "$@"
"""


class KeepChecksTests(unittest.TestCase):
    def test_maturin_with_a_long_banner_is_kept(self):
        code, out, log = run_check(
            "maturin",
            {"python3": "exit 0\n",
             "pip3": PIP3_STUB.replace("VERSION_LINE", "Version: 1.15.0")
                              .replace("PADDING", PADDING)},
            {"MATURIN_VERSION": "1.15.0"},
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn("maturin 1.15.0 (already present)", out)
        self.assertNotIn("installing maturin", out)
        self.assertNotIn("INSTALL", log)

    def test_maturin_at_the_wrong_version_still_installs(self):
        code, out, log = run_check(
            "maturin",
            {"python3": "exit 0\n",
             "pip3": PIP3_STUB.replace("VERSION_LINE", "Version: 9.9.9")
                              .replace("PADDING", PADDING)},
            {"MATURIN_VERSION": "1.15.0"},
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn("installing maturin==1.15.0", out)
        self.assertIn("INSTALL", log)

    def test_rust_toolchain_with_a_long_banner_is_kept(self):
        code, out, log = run_check(
            "rust",
            {"rustup": RUSTUP_STUB.replace(
                 "TOOLCHAIN_LINE", "1.94.0-x86_64-unknown-linux-gnu (default)")
                 .replace("PADDING", PADDING),
             "rustc": 'echo "rustc 1.94.0 (stub)"\n',
             "cargo": 'echo "cargo 1.94.0 (stub)"\n'},
            {"rust_version": "1.94.0", "rust_targets": ""},
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertNotIn("installing rust", out)
        self.assertNotIn("INSTALL", log)

    def test_rust_cc_present_is_kept_without_apt(self):
        # Build scripts link with `cc` even for zig/musl targets, so
        # setup_rust ensures it on Linux -- and a present cc only logs.
        code, out, log = run_check(
            "rust",
            {"rustup": RUSTUP_STUB.replace(
                 "TOOLCHAIN_LINE", "1.94.0-x86_64-unknown-linux-gnu (default)")
                 .replace("PADDING", PADDING),
             "rustc": 'echo "rustc 1.94.0 (stub)"\n',
             "cargo": 'echo "cargo 1.94.0 (stub)"\n',
             "cc": "exit 0\n",
             "apt-get": 'echo "APT $*" >> "$STUB_LOG"\nexit 1\n'},
            {"rust_version": "1.94.0", "rust_targets": "", "OS": "Linux"},
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertRegex(out, r"(?m)cc .*\(already present\)")
        self.assertNotIn("build-essential", out)
        self.assertNotIn("APT", log)

    def test_rust_cc_missing_installs_build_essential(self):
        # No cc on PATH (isolated: the real PATH carries one on every
        # dev and CI machine) and an apt-get: the toolchain installs.
        code, out, log = run_check(
            "rust",
            {"rustup": RUSTUP_STUB.replace(
                 "TOOLCHAIN_LINE", "1.94.0-x86_64-unknown-linux-gnu (default)")
                 .replace("PADDING", PADDING),
             "rustc": 'echo "rustc 1.94.0 (stub)"\n',
             "cargo": 'echo "cargo 1.94.0 (stub)"\n',
             "apt-get": APT_STUB,
             "cut": CUT_STUB},
            {"rust_version": "1.94.0", "rust_targets": "", "OS": "Linux"},
            isolate_path=True,
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn("installing build-essential pkg-config", out)
        self.assertIn("APT update -qq", log)
        self.assertIn("APT install -y -qq build-essential pkg-config", log)

    def test_rust_cc_missing_without_apt_get_refuses(self):
        code, out, _log = run_check(
            "rust", {},
            {"rust_version": "1.94.0", "rust_targets": "", "OS": "Linux"},
            isolate_path=True,
        )
        self.assertEqual(code, 2, out[-2000:])
        self.assertIn("cc is missing", out)
        self.assertIn("no apt-get", out)

    def test_setup_rust_ensures_a_host_cc_on_linux(self):
        match = re.search(r"setup_rust\(\) \{(.*?)\n\}\n", CHECKS.read_text(), re.S)
        self.assertIsNotNone(match, "setup_rust is gone from _selfhosted_checks.sh")
        body = match.group(1)
        for needle in (
            '"${OS:-}" = "Linux"',
            "command -v cc",
            "(already present)",
            "build-essential",
            "pkg-config",
            "${SUDO:-} apt-get",
            "no apt-get",
        ):
            with self.subTest(needle):
                self.assertIn(needle, body)

    def test_dotnet_sdk_with_a_long_banner_is_kept(self):
        code, out, _log = run_check(
            "dotnet",
            {"dotnet": DOTNET_STUB.replace(
                 "SDK_LINE", "8.0.100 [/stub/dotnet/sdk]")
                 .replace("PADDING", PADDING),
             # The install path downloads; it must be unreachable here,
             # and fail fast rather than bill network if it ever is.
             "curl": "exit 1\n"},
            {"dotnet_version": "8"},
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertIn("(already present)", out)
        self.assertNotIn("installing dotnet", out)

    def test_napi_at_the_pin_is_present(self):
        code, out, log = run_probe(
            {"npm": npm_reporting("echo '%s'" % npm_json(NAPI_PIN)),
             "node": NODE_STUB},
            NAPI_PIN,
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertRegex(out, r"(?m)^present$")
        self.assertIn("ARGS ls -g @napi-rs/cli --depth=0 --json", log)

    def test_napi_at_another_version_is_not_present(self):
        code, out, log = run_probe(
            {"npm": npm_reporting("echo '%s'" % npm_json(NAPI_OTHER)),
             "node": NODE_STUB},
            NAPI_PIN,
        )
        self.assertEqual(code, 0, out[-2000:])
        self.assertRegex(out, r"(?m)^not present$")
        self.assertIn("ARGS ls -g @napi-rs/cli --depth=0 --json", log)

    def test_napi_absent_is_not_present_and_no_error(self):
        # npm ls exits 1 when the package is absent; the probe must
        # survive the miss under `set -euo pipefail`, empty or not.
        for label, body in (("empty json", "echo '{}'; exit 1"),
                            ("no output", "exit 1")):
            with self.subTest(label):
                code, out, log = run_probe(
                    {"npm": npm_reporting(body), "node": NODE_STUB},
                    NAPI_PIN,
                )
                self.assertEqual(code, 0, out[-2000:])
                self.assertRegex(out, r"(?m)^not present$")
                self.assertIn("ARGS ls -g @napi-rs/cli --depth=0 --json", log)

    def test_old_napi_version_probe_misses_the_installed_pin(self):
        # The red-proof: the pin is installed (npm reports it) and `napi`
        # is the 2.x binary, which answers --version with "Unsupported
        # option name". The old probe form reports "not present" against
        # that fixture -- the every-run reinstall from the Windows smoke;
        # the npm probe reports present.
        stubs = {
            "napi": NAPI2_STUB,
            "npm": npm_reporting("echo '%s'" % npm_json(NAPI_PIN)),
            "node": NODE_STUB,
        }
        code, out, _log = _run(OLD_NAPI_DRIVER, NAPI_PIN, stubs, {})
        self.assertEqual(code, 0, out[-2000:])
        self.assertRegex(out, r"(?m)^not present$")
        code, out, _log = _run(NAPI_DRIVER, NAPI_PIN, stubs, {})
        self.assertEqual(code, 0, out[-2000:])
        self.assertRegex(out, r"(?m)^present$")

    def test_setup_napi_reads_the_installed_version_from_npm(self):
        text = SETUP.read_text()
        for lineno, line in enumerate(text.splitlines(), 1):
            code = line.split("#", 1)[0]
            self.assertNotIn(
                "napi --version", code,
                "selfhosted_setup.sh line %d still probes the 2.x binary"
                % lineno)
        match = re.search(r"setup_napi\(\) \{(.*?)\n\}\n", text, re.S)
        self.assertIsNotNone(match, "setup_napi is gone from selfhosted_setup.sh")
        self.assertIn(
            "napi_installed_version", match.group(1),
            "setup_napi no longer reads the installed version from npm")

    def test_docker_group_refusal_names_the_pre_start_fix(self):
        match = re.search(r"setup_docker\(\) \{(.*?)\n\}\n", SETUP.read_text(), re.S)
        self.assertIsNotNone(match, "setup_docker is gone from selfhosted_setup.sh")
        body = match.group(1)
        self.assertIn(
            "the runner must be in the docker group before the runner process starts",
            body,
        )
        self.assertIn("gcp_runner.sh", body)
        self.assertLess(
            body.index("$SUDO docker info"),
            body.index("before the runner process starts"),
            "the sudo-only probe guards the refusal",
        )
        code = "\n".join(line.split("#", 1)[0] for line in body.splitlines())
        self.assertNotIn(
            "chmod", code, "no socket chmod: it cannot fix the running process"
        )

    def test_no_grep_q_pipeline_in_the_setup(self):
        for script in (SETUP, CHECKS):
            text = script.read_text()
            for lineno, line in enumerate(text.splitlines(), 1):
                code = line.split("#", 1)[0]
                self.assertNotIn(
                    "grep", code,
                    "%s line %d keeps a grep in a pipeline"
                    % (script.name, lineno))


if __name__ == "__main__":
    unittest.main()
