#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The Linux python tools run from a versioned venv, and tool dirs persist.

Ubuntu 24.04 splits ensurepip and pip out of the base python into the
python3.X-venv / python3-pip packages, and refuses pip installs into the
system python anyway (PEP 668). So on Linux selfhosted_setup.sh installs
the -venv package through sudo -n when venv or ensurepip is unavailable,
creates $HOME/tools/py<major><minor> once, and installs maturin and zig
through the venv's pip -- never pip3 or the system site-packages. When
apt has no such minor (24.04 ships 3.12 only) the interpreter comes from
a pinned python-build-standalone release instead. And every existing
tool dir joins PATH before the prerequisite checks, so a second setup
step finds what an earlier one installed.

Each case runs the real script with stubbed tools in a temp HOME, with
uname stubbed to Linux: the venv-less python3.12 gains its venv only
after the stubbed apt-get "installs" it, the standalone build arrives
as a local tarball, and cargo hides off PATH in $HOME/.cargo/bin.
SELFHOSTED_SETUP_UNDER_TEST runs a variant script instead, which is
how the red proofs below run against the pre-venv script.
"""
from __future__ import annotations

import hashlib
import io
import os
import pathlib
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = pathlib.Path(
    os.environ.get(
        "SELFHOSTED_SETUP_UNDER_TEST",
        REPO / "scripts/release/selfhosted_setup.sh",
    )
)

sys.path.insert(0, str(REPO / "scripts" / "gcp"))
import check_pip_pins  # noqa: E402

UNAME_LINUX = 'if [ "$1" = "-s" ]; then echo Linux; else echo x86_64; fi\n'
ID_USER = "echo 501\n"
CURL_REFUSE = 'echo "CURL $*" >> "$STUB_LOG"\nexit 1\n'
APT_GET_STUB = "exit 0\n"  # existence only; every call goes through sudo
APT_CACHE_MISS = "exit 1\n"

SUDO_OK = """\
echo "SUDO $*" >> "$STUB_LOG"
if [ "$1" = "-n" ]; then shift; fi
if [ "$1" = "apt-get" ] && [ "$2" = "install" ]; then touch "$STUB_APT_DONE"; fi
exit 0
"""

SUDO_REFUSE = """\
echo "SUDO $*" >> "$STUB_LOG"
echo "sudo: a password is required" >&2
exit 1
"""

# A 24.04 python3.12: no venv or ensurepip until apt "installs" the -venv
# package (the marker), and a -m venv that lands a pip like the real one.
PYTHON312_NOVENV = """\
if [ "$1" = "-m" ] && [ "$2" = "venv" ] && [ "$3" = "--help" ]; then
  if [ -f "$STUB_APT_DONE" ]; then exit 0; else exit 1; fi
fi
if [ "$1" = "-c" ]; then
  if [ -f "$STUB_APT_DONE" ]; then exit 0; else echo "No module named ensurepip" >&2; exit 1; fi
fi
if [ "$1" = "-m" ] && [ "$2" = "venv" ]; then
  dir=""; for a in "$@"; do dir="$a"; done
  mkdir -p "$dir/bin"
  cp "$STUB_PIP" "$dir/bin/pip"
  chmod +x "$dir/bin/pip"
  exit 0
fi
if [ "$1" = "--version" ]; then echo "Python 3.12.3"; exit 0; fi
if [ "$1" = "-m" ] && [ "$2" = "pip" ]; then echo "pip 25.0.0 from stub"; exit 0; fi
exit 0
"""

# The venv's pip: --version for the final log, a wrong version for show so
# the install always runs, and every install on the stub log.
VENV_PIP = """\
if [ "$1" = "--version" ]; then echo "pip 25.0.0 from stub-venv"; exit 0; fi
if [ "$1" = "show" ]; then echo "Name: $2"; echo "Version: 0.0.0"; exit 0; fi
echo "PIP $*" >> "$STUB_LOG"
exit 0
"""

PIP3_SCREAM = 'echo "PIP3-CALLED $*" >> "$STUB_LOG"\nexit 3\n'

# A python3 of the wrong minor: present for need_tool, never selected.
PYTHON3_WRONG_MINOR = 'if [ "$1" = "-c" ]; then exit 1; fi\nexit 0\n'

STANDALONE_PYTHON311 = """\
if [ "$1" = "-m" ] && [ "$2" = "venv" ] && [ "$3" = "--help" ]; then exit 0; fi
if [ "$1" = "-c" ]; then exit 0; fi
if [ "$1" = "-m" ] && [ "$2" = "venv" ]; then
  dir=""; for a in "$@"; do dir="$a"; done
  mkdir -p "$dir/bin"
  cp "$STUB_PIP" "$dir/bin/pip"
  chmod +x "$dir/bin/pip"
  exit 0
fi
if [ "$1" = "--version" ]; then echo "Python 3.11.17"; exit 0; fi
exit 0
"""

CARGO_STUB = 'echo "CARGO $*" >> "$STUB_LOG"\nexit 0\n'
WASM_PACK_STUB = 'echo "wasm-pack 0.13.1"\n'
WASM_BINDGEN_STUB = 'echo "wasm-bindgen 0.2.99"\n'

CARGO_LOCK = """\
[[package]]
name = "wasm-bindgen"
version = "0.2.99"
"""


class VenvCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="pyvenv-")
        self.addCleanup(self.tmp.cleanup)
        tmp = pathlib.Path(self.tmp.name)
        self.home = tmp / "home"
        self.home.mkdir()
        self.bindir = tmp / "bin"
        self.bindir.mkdir()
        self.log = tmp / "stub.log"
        self.log.write_text("")
        self.bash = shutil.which("bash")
        self.assertIsNotNone(self.bash, "bash is missing")
        self.venv_pip = tmp / "venv-pip"
        self.venv_pip.write_text("#!/bin/sh\n" + VENV_PIP)
        self.venv_pip.chmod(0o755)
        self.apt_done = tmp / "apt-done"

    def build_path(self, hidden: frozenset[str], stubs: dict[str, str]):
        """A PATH without hidden, with every stub on it."""
        for entry in os.environ.get("PATH", "").split(os.pathsep):
            if not entry or not os.path.isdir(entry):
                continue
            for name in sorted(os.listdir(entry)):
                if name in hidden or (self.bindir / name).exists():
                    continue
                source = os.path.join(entry, name)
                if os.path.isdir(source) and not os.path.islink(source):
                    continue
                try:
                    os.symlink(source, self.bindir / name)
                except FileExistsError:
                    pass
        for name, body in stubs.items():
            stub = self.bindir / name
            if stub.exists() or stub.is_symlink():
                stub.unlink()
            stub.write_text("#!/bin/sh\n" + body)
            stub.chmod(stub.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    def run_setup(self, *args: str, script: pathlib.Path | None = None,
                  cwd: pathlib.Path | None = None,
                  extra_env: dict[str, str] | None = None,
                  ) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env["HOME"] = str(self.home)
        env["PATH"] = str(self.bindir)
        env["STUB_LOG"] = str(self.log)
        env["STUB_PIP"] = str(self.venv_pip)
        env["STUB_APT_DONE"] = str(self.apt_done)
        env.pop("GITHUB_PATH", None)
        env.pop("GITHUB_ENV", None)
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            [self.bash, str(script or SCRIPT), *args],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=cwd or REPO,
            env=env,
        )

    def out(self, proc: subprocess.CompletedProcess[str]) -> str:
        return proc.stdout + proc.stderr

    def test_apt_venv_package_then_venv(self):
        """A venv-less python3.12 gains -venv through sudo -n, then a venv."""
        self.build_path(
            frozenset({"curl", "unzip", "python3", "python3.11", "python3.12",
                       "pip3", "sudo", "apt-get", "apt-cache", "uname", "id"}),
            {"uname": UNAME_LINUX, "id": ID_USER,
             "curl": CURL_REFUSE, "unzip": APT_GET_STUB,
             "sudo": SUDO_OK, "apt-get": APT_GET_STUB,
             "python3": PYTHON3_WRONG_MINOR, "python3.12": PYTHON312_NOVENV},
        )
        github_path = pathlib.Path(self.tmp.name) / "github-path"
        github_path.write_text("")
        proc = self.run_setup("--tools", "python", "--python-version", "3.12",
                              extra_env={"GITHUB_PATH": str(github_path)})
        self.assertEqual(proc.returncode, 0, self.out(proc)[-2000:])
        self.assertIn("installing python3.12-venv python3-pip", self.out(proc))
        calls = self.log.read_text()
        self.assertIn(
            "SUDO -n apt-get install -y -qq python3.12-venv python3-pip", calls)
        venv_pip = self.home / "tools" / "py312" / "bin" / "pip"
        self.assertTrue(venv_pip.is_file(), "the venv carries its own pip")
        self.assertIn(str(venv_pip.parent), github_path.read_text().splitlines())
        self.assertIn("creating the python 3.12 venv", self.out(proc))

        # Idempotent: the second run installs and creates nothing.
        again = self.run_setup("--tools", "python", "--python-version", "3.12",
                               extra_env={"GITHUB_PATH": str(github_path)})
        self.assertEqual(again.returncode, 0, self.out(again)[-2000:])
        self.assertNotIn("installing python3.12-venv", self.out(again))
        self.assertNotIn("creating the python 3.12 venv", self.out(again))

    def test_maturin_installs_through_the_venv_pip(self):
        """maturin uses "$PIP" from the venv; pip3 is never called."""
        self.build_path(
            frozenset({"curl", "unzip", "python3", "python3.11", "python3.12",
                       "pip3", "sudo", "apt-get", "apt-cache", "uname", "id"}),
            {"uname": UNAME_LINUX, "id": ID_USER,
             "curl": CURL_REFUSE, "unzip": APT_GET_STUB,
             "sudo": SUDO_OK, "apt-get": APT_GET_STUB,
             "python3": PYTHON3_WRONG_MINOR, "python3.12": PYTHON312_NOVENV,
             "pip3": PIP3_SCREAM},
        )
        proc = self.run_setup("--tools", "python,maturin",
                              "--python-version", "3.12",
                              "--maturin-version", "1.15.0")
        self.assertEqual(proc.returncode, 0, self.out(proc)[-2000:])
        self.assertIn("installing maturin==1.15.0", self.out(proc))
        calls = self.log.read_text()
        self.assertIn("PIP install --quiet maturin==1.15.0", calls)
        self.assertNotIn("PIP3-CALLED", calls)

    def test_refuses_without_passwordless_sudo(self):
        """No passwordless sudo: the python step refuses, naming the package."""
        self.build_path(
            frozenset({"curl", "unzip", "python3", "python3.11", "python3.12",
                       "pip3", "sudo", "apt-get", "apt-cache", "uname", "id"}),
            {"uname": UNAME_LINUX, "id": ID_USER,
             "curl": CURL_REFUSE, "unzip": APT_GET_STUB,
             "sudo": SUDO_REFUSE, "apt-get": APT_GET_STUB,
             "python3": PYTHON3_WRONG_MINOR, "python3.12": PYTHON312_NOVENV},
        )
        proc = self.run_setup("--tools", "python", "--python-version", "3.12")
        self.assertEqual(proc.returncode, 2, self.out(proc)[-2000:])
        self.assertIn("python3.12-venv", self.out(proc))

    def test_second_run_finds_cargo_off_path(self):
        """A later --tools wasm run finds $HOME/.cargo/bin off the PATH."""
        self.build_path(
            frozenset({"curl", "unzip", "cargo", "apt-get", "sudo", "uname"}),
            {"uname": UNAME_LINUX, "id": ID_USER,
             "curl": CURL_REFUSE, "unzip": APT_GET_STUB,
             "sudo": SUDO_OK, "apt-get": APT_GET_STUB,
             "wasm-pack": WASM_PACK_STUB, "wasm-bindgen": WASM_BINDGEN_STUB},
        )
        cargo_dir = self.home / ".cargo" / "bin"
        cargo_dir.mkdir(parents=True)
        cargo = cargo_dir / "cargo"
        cargo.write_text("#!/bin/sh\n" + CARGO_STUB)
        cargo.chmod(0o755)
        repo = pathlib.Path(self.tmp.name) / "repo"
        repo.mkdir()
        (repo / "Cargo.lock").write_text(CARGO_LOCK)
        proc = self.run_setup("--tools", "wasm", cwd=repo)
        self.assertEqual(proc.returncode, 0, self.out(proc)[-2000:])
        self.assertNotIn("cargo is missing", self.out(proc))
        self.assertIn(str(cargo_dir), self.out(proc))

    def test_standalone_python_when_the_minor_is_not_in_apt(self):
        """3.11 with apt knowing no python3.11 comes from standalone."""
        self.build_path(
            frozenset({"curl", "unzip", "python3", "python3.11", "python3.12",
                       "pip3", "sudo", "apt-get", "apt-cache", "uname", "id"}),
            {"uname": UNAME_LINUX, "id": ID_USER,
             "curl": CURL_REFUSE, "unzip": APT_GET_STUB,
             "sudo": SUDO_OK, "apt-get": APT_GET_STUB,
             "apt-cache": APT_CACHE_MISS, "python3": PYTHON3_WRONG_MINOR},
        )
        tarball = pathlib.Path(self.tmp.name) / "standalone.tar.gz"
        body = ("#!/bin/sh\n" + STANDALONE_PYTHON311).encode()
        with tarfile.open(tarball, "w:gz") as tar:
            info = tarfile.TarInfo("python/bin/python3.11")
            info.size = len(body)
            info.mode = 0o755
            info.mtime = 1700000000
            tar.addfile(info, fileobj=io.BytesIO(body))
        text = SCRIPT.read_text()
        patched = text.replace(
            'fetch "$standalone_url" "$tmp/python.tar.gz"',
            'cp "$STANDALONE_TARBALL_FIXTURE" "$tmp/python.tar.gz"')
        self.assertNotEqual(patched, text, "the standalone download moved")
        sha = hashlib.sha256(tarball.read_bytes()).hexdigest()
        patched, count = re.subn(
            r'(3\.11-x86_64\) st_ver="[^"]+"; st_cpu="[^"]+"; st_sha=")[0-9a-f]{64}(")',
            r"\g<1>" + sha + r"\g<2>", patched)
        self.assertEqual(count, 1, "the 3.11 x86_64 pin moved")
        variant = pathlib.Path(self.tmp.name) / "selfhosted_setup.sh"
        variant.write_text(patched)
        # The variant sources its checks from beside itself, as the repo
        # script does; a pre-split variant never sources, so the extra
        # file is harmless there.
        (variant.parent / "_selfhosted_checks.sh").write_text(
            (REPO / "scripts" / "release" / "_selfhosted_checks.sh")
            .read_text())
        proc = self.run_setup("--tools", "python", "--python-version", "3.11",
                              script=variant,
                              extra_env={"STANDALONE_TARBALL_FIXTURE":
                                         str(tarball)})
        self.assertEqual(proc.returncode, 0, self.out(proc)[-2000:])
        self.assertIn("(standalone)", self.out(proc))
        venv_pip = self.home / "tools" / "py311" / "bin" / "pip"
        self.assertTrue(venv_pip.is_file(), "the venv is made from standalone")
        self.assertNotIn("install -y -qq python3.11", self.log.read_text())

    def test_pip_installs_keep_the_checkable_form(self):
        """The checker's own parser still reads every "$PIP" install."""
        texts = [SCRIPT.read_text()]
        sibling = SCRIPT.parent / "_selfhosted_checks.sh"
        if sibling.is_file():
            texts.insert(0, sibling.read_text())
        reqs = []
        for text in texts:
            reqs += check_pip_pins.extract_groups(text)["base"]
        self.assertEqual(reqs,
                         ["maturin==1.15.0",
                          "cargo-zigbuild==0.23.4",
                          "ziglang==0.16.0"])

    def test_standalone_builds_are_pinned(self):
        """One standalone tag, and a sha256 per Linux build."""
        text = SCRIPT.read_text()
        self.assertIn('PYTHON_STANDALONE_TAG="20261003"', text)
        self.assertIn(
            "python-build-standalone/releases/download/${PYTHON_STANDALONE_TAG}",
            text)
        for minor, cpu in (("3.11", "x86_64"), ("3.11", "aarch64"),
                           ("3.12", "x86_64"), ("3.12", "aarch64")):
            found = re.search(
                r'%s-(?:%s\|[^)]*|%s)\) st_ver="[^"]+"; st_cpu="%s"; '
                r'st_sha="([0-9a-f]{64})"' % (minor, cpu, cpu, cpu), text)
            self.assertIsNotNone(
                found, "no sha256 pin for %s/%s" % (minor, cpu))

    def test_no_bare_pip3_outside_the_default(self):
        """Only PIP="pip3" names pip3; the installs use "$PIP"."""
        texts = [SCRIPT.read_text()]
        sibling = SCRIPT.parent / "_selfhosted_checks.sh"
        if sibling.is_file():
            texts.append(sibling.read_text())
        named = [line for text in texts for line in text.splitlines()
                 if "pip3" in line.split("#", 1)[0]]
        self.assertEqual(named, ['PIP="pip3"'])


if __name__ == "__main__":
    unittest.main()

