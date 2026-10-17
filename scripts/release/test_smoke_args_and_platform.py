#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""One argument contract for every smoke, held on every smoke's parser.

--artifact names the file under test (a missing one fails naming the path),
--platform must name this host unless --cross-check says otherwise, and an
unknown flag exits 2 naming itself. The contract lives in
scripts/release/consumer_smokes/smoke_common.sh, which every smoke sources;
every case below runs every smoke's own parser, so a smoke that drifts off
the contract fails here under its own name.

What each leg used to do: smoke_dotnet and smoke_wasm ended their parsing
on `*) shift`, so the runner's --artifact was silently swallowed and the
smoke vouched for whatever the defaults picked; smoke_rust, smoke_node and
smoke_binary rejected --platform outright, so a runner pass with --platform
broke them; smoke_cli recorded --platform without judging it.
"""
from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys
import glob
import hashlib
import zipfile
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SMOKE_DIR = REPO / "scripts" / "release" / "consumer_smokes"
SMOKES = {
    "python": SMOKE_DIR / "smoke_python.sh",
    "java": SMOKE_DIR / "smoke_java.sh",
    "dotnet": SMOKE_DIR / "smoke_dotnet.sh",
    "rust": SMOKE_DIR / "smoke_rust.sh",
    "wasm": SMOKE_DIR / "smoke_wasm.sh",
    "node": SMOKE_DIR / "smoke_node.sh",
    "binary": SMOKE_DIR / "smoke_binary.sh",
    "cli": SMOKE_DIR / "smoke_cli.sh",
    "capi": SMOKE_DIR / "smoke_capi.sh",
    "ort": SMOKE_DIR / "smoke_ort_variant.sh",
}
COMMON = SMOKE_DIR / "smoke_common.sh"

MINIMUM_CASES = 7  # FLOOR

MISSING = {
    "python": "/nonexistent/pdfluent-9.9.9-py3-none-any.whl",
    "java": "/nonexistent/pdfluent-9.9.9.jar",
    "dotnet": "/nonexistent/PDFluent-9.9.9.nupkg",
    "rust": "/nonexistent/pdfluent-9.9.9.crate",
    "wasm": "/nonexistent/pdfluent-wasm-pkg",
    "node": "/nonexistent/pdfluent-node-9.9.9.tgz",
    "binary": "/nonexistent/pdfluent-9.9.9-x86_64-unknown-linux-gnu.tar.gz",
    "cli": "/nonexistent/pdfluent-9.9.9-x86_64-unknown-linux-gnu.tar.gz",
    "capi": "/nonexistent/pdfluent-c-9.9.9-osx-arm64.tar.gz",
    "ort": "/nonexistent/pdfluent-9.9.9-x86_64-unknown-linux-gnu.tar.gz",
}


def _run(name: str, *args: str, release_lane: bool = False) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    if release_lane:
        env["PDFLUENT_RELEASE_LANE"] = "1"
    else:
        env.pop("PDFLUENT_RELEASE_LANE", None)
    return subprocess.run(
        ["bash", str(SMOKES[name]), *args],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=REPO,
        env=env,
    )


_HOST_KEYS: dict[str, str] = {}


def _host_key(name: str) -> str:
    """This host's key in the smoke's own spelling, read off its mismatch line."""
    if name not in _HOST_KEYS:
        proc = _run(name, "--platform", "__probe__", "--artifact", MISSING[name])
        m = re.search(r"this host is (\S+)", proc.stdout + proc.stderr)
        assert m, proc.stdout + proc.stderr
        _HOST_KEYS[name] = m.group(1)
    return _HOST_KEYS[name]


class SmokeArgsTests(unittest.TestCase):
    def test_every_smoke_sources_the_shared_contract(self):
        # The contract has one implementation: a smoke that parses the
        # three flags with its own copy drifts, so each one sources it.
        self.assertTrue(COMMON.is_file(), f"missing {COMMON}")
        for name, script in SMOKES.items():
            with self.subTest(smoke=name):
                self.assertIn("smoke_common.sh", script.read_text())

    def test_unknown_option_is_an_error_naming_the_flag(self):
        for name in SMOKES:
            with self.subTest(smoke=name):
                proc = _run(name, "--bogus-flag", release_lane=True)
                self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
                self.assertIn("unknown option", proc.stderr)
                self.assertIn("--bogus-flag", proc.stderr)

    def test_platform_mismatch_is_refused_naming_the_host(self):
        for name in SMOKES:
            with self.subTest(smoke=name):
                proc = _run(
                    name, "--platform", "__probe__",
                    "--artifact", MISSING[name],
                )
                self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
                self.assertIn("platform mismatch", proc.stderr)
                self.assertIn("--cross-check", proc.stderr)

    def test_cross_check_runs_past_the_platform_check(self):
        for name in SMOKES:
            with self.subTest(smoke=name):
                proc = _run(
                    name, "--platform", "__probe__", "--cross-check",
                    "--artifact", MISSING[name],
                )
                self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
                self.assertNotIn("platform mismatch", proc.stdout + proc.stderr)
                self.assertIn(MISSING[name], proc.stdout + proc.stderr)

    def test_matching_platform_proceeds(self):
        for name in SMOKES:
            with self.subTest(smoke=name):
                proc = _run(
                    name, "--platform", _host_key(name),
                    "--artifact", MISSING[name],
                )
                self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
                self.assertNotIn("platform mismatch", proc.stdout + proc.stderr)
                self.assertIn(MISSING[name], proc.stdout + proc.stderr)

    def test_platform_aliases_match_the_host(self):
        for name in SMOKES:
            with self.subTest(smoke=name):
                host = _host_key(name)
                alias = (host.replace("osx-", "macos-")
                             .replace("x64", "x86_64")
                             .replace("arm64", "aarch64"))
                if alias == host:
                    self.skipTest(f"host key {host} has no alias spelling")
                proc = _run(
                    name, "--platform", alias,
                    "--artifact", MISSING[name],
                )
                self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
                self.assertNotIn("platform mismatch", proc.stdout + proc.stderr)

    def test_no_platform_skips_the_check(self):
        for name in SMOKES:
            with self.subTest(smoke=name):
                proc = _run(name, "--artifact", MISSING[name])
                self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
                self.assertNotIn("platform mismatch", proc.stdout + proc.stderr)


class SmokeWasmPkgDirTests(unittest.TestCase):
    """smoke_wasm.sh installs the package it was given, and says so.

    Build 2: the smoke cd'd into a temp dir and ran `npm install` with
    the still-relative --pkg-dir, so the path no longer resolved — and
    --silent hid npm's error.
    """

    def _stubbed_run(self, root: pathlib.Path, *args: str):
        bindir = root / "bin"
        bindir.mkdir()
        log = root / "stub.log"
        log.write_text("")
        (bindir / "npm").write_text('#!/bin/sh\necho "NPM $*" >> "$STUB_LOG"\nexit 0\n')
        (bindir / "node").write_text('#!/bin/sh\necho "NODE $*" >> "$STUB_LOG"\nexit 0\n')
        (bindir / "npm").chmod(0o755)
        (bindir / "node").chmod(0o755)
        env = dict(
            os.environ,
            PATH=str(bindir) + os.pathsep + os.environ["PATH"],
            STUB_LOG=str(log),
        )
        env.pop("PDFLUENT_RELEASE_LANE", None)
        proc = subprocess.run(
            ["bash", str(SMOKES["wasm"]), *args],
            capture_output=True, text=True, timeout=120, cwd=root, env=env,
        )
        return proc, log.read_text()

    def test_relative_pkg_dir_is_installed_by_absolute_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            pkg = root / "relpkg"
            pkg.mkdir()
            (pkg / "package.json").write_text(
                '{"name": "@pdfluent/sdk-wasm", "version": "1.2.3"}\n'
            )
            proc, log = self._stubbed_run(root, "--pkg-dir", "relpkg")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            installs = [l for l in log.splitlines() if l.startswith("NPM install")]
            self.assertEqual(len(installs), 1, log)
            installed = installs[0].split()[-1]
            self.assertTrue(
                os.path.isabs(installed),
                f"npm installed a relative path: {installed}",
            )
            self.assertEqual(installed, os.path.realpath(pkg))
            self.assertIn("NODE", log)

    def test_npm_runs_loud(self):
        # --silent hid npm's error in build 2: the install shows its
        # output now.
        code = "\n".join(
            line.split("#", 1)[0]
            for line in SMOKES["wasm"].read_text().splitlines()
        )
        self.assertNotIn("--silent", code)
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            pkg = root / "relpkg"
            pkg.mkdir()
            (pkg / "package.json").write_text(
                '{"name": "@pdfluent/sdk-wasm", "version": "1.2.3"}\n'
            )
            _proc, log = self._stubbed_run(root, "--pkg-dir", "relpkg")
            self.assertNotIn("--silent", log)

    def test_explicit_missing_pkg_dir_fails_instead_of_skipping(self):
        proc = _run("wasm", "--pkg-dir", "/nonexistent/wasm-pkg-636")
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("/nonexistent/wasm-pkg-636", proc.stdout + proc.stderr)
        self.assertNotIn("SKIPPED", proc.stdout + proc.stderr)

    def test_no_argument_without_a_pkg_skips_loudly(self):
        # The skip stays for the no-argument local-dev case only, and it
        # says so loudly.
        for candidate in ("crates/xfa-wasm/pkg", "crates/xfa-wasm/target/pkg"):
            if (REPO / candidate).is_dir():
                self.skipTest(f"{candidate} is built in this checkout")
        proc = _run("wasm")
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("SMOKE SKIPPED", proc.stdout + proc.stderr)
        self.assertIn("--pkg-dir", proc.stdout + proc.stderr)


class SmokeHostLibcTests(unittest.TestCase):
    """The host key names musl on a musl host, under the callers' pipefail.

    r9d: inside alpine:3.22 the linux-x64-musl smoke read its host as
    linux-x64 and refused itself. musl's `ldd --version` prints its banner
    and exits 1, so `ldd --version | grep -qi musl` failed under pipefail
    even though grep matched.
    """

    def _host(self, ldd_body: str) -> str:
        with tempfile.TemporaryDirectory() as tmp:
            bindir = pathlib.Path(tmp)
            (bindir / "uname").write_text(
                '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n'
            )
            (bindir / "ldd").write_text("#!/bin/sh\n" + ldd_body)
            for f in ("uname", "ldd"):
                (bindir / f).chmod(0o755)
            env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"])
            proc = subprocess.run(
                ["bash", "-c", f'set -Eeuo pipefail; source "{COMMON}"; smoke_host_platform'],
                capture_output=True, text=True, timeout=60, env=env,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            return proc.stdout.strip()

    def test_musl_ldd_that_exits_one_reads_as_musl(self):
        self.assertEqual(
            self._host('echo "musl libc (x86_64)" >&2\necho "Version 1.2.5" >&2\nexit 1\n'),
            "linux-x64-musl",
        )

    def test_glibc_ldd_reads_as_plain_linux(self):
        if glob.glob("/lib/ld-musl-*.so.1"):
            print("SKIPPED (not a pass): this host carries a musl loader", file=sys.stderr)
            self.skipTest("musl loader present on this host")
        self.assertEqual(
            self._host('echo "ldd (GNU libc) 2.39"\nexit 0\n'), "linux-x64"
        )


class SmokeNodeTarballPathTests(unittest.TestCase):
    """smoke_node.sh installs the tarballs it was given, by absolute path.

    r9d osx-x64: NODE_TGZ was `nodetgz/...`; after the `cd` into the scratch
    project npm read it as a GitHub shorthand (git ls-remote
    ssh://git@github.com/nodetgz/...), and --silent hid that.
    """

    def test_relative_tarballs_are_installed_by_absolute_path_and_loud(self):
        code = "\n".join(
            line.split("#", 1)[0] for line in SMOKES["node"].read_text().splitlines()
        )
        self.assertNotIn("--silent", code)
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            bindir = root / "bin"
            bindir.mkdir()
            log = root / "stub.log"
            log.write_text("")
            # npm records its argv and fails, so the smoke stops right after
            # the install it is asked about.
            (bindir / "npm").write_text(
                '#!/bin/sh\n[ "$1" = --version ] && { echo 10.0.0; exit 0; }\n'
                'echo "NPM $*" >> "$STUB_LOG"\nexit 1\n'
            )
            (bindir / "node").write_text("#!/bin/sh\necho v24.0.0\n")
            for f in ("npm", "node"):
                (bindir / f).chmod(0o755)
            tgz = root / "nodetgz"
            tgz.mkdir()
            for name in ("pdfluent-node-9.9.9.tgz", "pdfluent-node-linux-x64-gnu-9.9.9.tgz"):
                (tgz / name).write_bytes(b"")
            env = dict(
                os.environ,
                PATH=str(bindir) + os.pathsep + os.environ["PATH"],
                STUB_LOG=str(log),
            )
            env.pop("PDFLUENT_RELEASE_LANE", None)
            proc = subprocess.run(
                ["bash", str(SMOKES["node"]),
                 "--artifact", "nodetgz/pdfluent-node-9.9.9.tgz",
                 "--platform-pkg", "nodetgz/pdfluent-node-linux-x64-gnu-9.9.9.tgz",
                 "--version", "9.9.9"],
                capture_output=True, text=True, timeout=120, cwd=root, env=env,
            )
            out = proc.stdout + proc.stderr
            self.assertIn("FAIL: npm install of local tgz failed", out)
            installs = [l for l in log.read_text().splitlines() if l.startswith("NPM install")]
            self.assertEqual(len(installs), 1, out)
            self.assertNotIn("--silent", installs[0])
            given = [a for a in installs[0].split() if a.endswith(".tgz")]
            self.assertEqual(
                given,
                [os.path.realpath(tgz / "pdfluent-node-9.9.9.tgz"),
                 os.path.realpath(tgz / "pdfluent-node-linux-x64-gnu-9.9.9.tgz")],
            )


class ReleaseNativesWindowsCrlfTests(unittest.TestCase):
    """release-natives' artefact download survives a CRLF-printing Python.

    r9d win-x64: Windows Python prints CRLF, `while read -r aid aname` kept
    the CR, and zipfile refused `nodearts\\pdf-node-release-...\r`. The run id
    from newest_successful_run carried the same CR into the next URL.
    """

    WORKFLOW = REPO / ".github" / "workflows" / "release-natives.yml"

    def _functions(self) -> str:
        text = self.WORKFLOW.read_text()
        start = text.index("          api_to() {")
        end = text.index("          NODE_RUN=", start)
        return "\n".join(line[10:] if line.startswith(" " * 10) else line
                         for line in text[start:end].splitlines())

    def test_run_id_and_artefact_names_carry_no_cr(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            log = root / "curl.log"
            # Windows-like python: the real interpreter, every stdout line ending in CRLF
            (root / "py").write_text('#!/bin/sh\nexec python3 "$@" | sed "s/$/\\r/"\n')
            # fake curl: records its URL, answers by route
            (root / "curl").write_text(
                '#!/bin/sh\nout=""; url=""\n'
                'while [ $# -gt 0 ]; do case "$1" in -o) out="$2"; shift 2;; -H|--retry|--retry-delay|--connect-timeout|--max-time) shift 2;; -*) shift;; *) url="$1"; shift;; esac; done\n'
                'printf "%s\\n" "$url" >> "$CURL_LOG"\n'
                'case "$url" in\n'
                '  */runs\\?*) echo \'{"workflow_runs":[{"id":4242}]}\' > "$out";;\n'
                '  */artifacts\\?*) printf \'{"artifacts":[{"id":7,"name":"pdf-node-release-x","digest":"sha256:%s"}]}\' "$ZIP_SHA" > "$out";;\n'
                '  */zip) cp "$ROOT/a.zip" "$out";;\n'
                'esac\n')
            for f in ("py", "curl"):
                (root / f).chmod(0o755)
            # The artefact zip and its digest: the download is checked against it.
            with zipfile.ZipFile(root / "a.zip", "w") as z:
                z.writestr("a.txt", "x")
            zip_sha = hashlib.sha256((root / "a.zip").read_bytes()).hexdigest()
            script = ("set -euo pipefail\nPY=\"$ROOT/py\"\n" + self._functions() +
                      '\nr="$(newest_successful_run node-bindings.yml)"\nprintf "RUN=[%s]\\n" "$r"\n'
                      'download_matching "$r" "pdf-node-release-" "$ROOT/arts"\nls "$ROOT/arts"\n')
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"], ROOT=str(root),
                       CURL_LOG=str(log), GH_TOKEN="t", GITHUB_API_URL="https://api.invalid",
                       GITHUB_REPOSITORY="o/r", REF="b", ZIP_SHA=zip_sha, RUNNER_TEMP=str(root))
            proc = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=60,
                                  env=env, cwd=REPO)
            out = proc.stdout + proc.stderr
            self.assertEqual(proc.returncode, 0, out)
            self.assertIn("RUN=[4242]", out)
            self.assertIn("pdf-node-release-x", out)
            self.assertNotIn("\r", log.read_text(), "a CR reached a URL")
            self.assertTrue((root / "arts" / "pdf-node-release-x").is_dir(), out)


class ReleaseNativesRosettaTests(unittest.TestCase):
    """Every smoke in the osx-x64 leg runs under the Rosetta wrapper.

    Run 38693729360: the runner's bash resolves to the arm64 /bin/bash, so
    `Smoke cli`, the one host smoke without the wrapper, ran natively and
    refused `--platform osx-x64` because this host is osx-arm64.
    """

    WORKFLOW = REPO / ".github" / "workflows" / "release-natives.yml"
    INVOKE = re.compile(r"^\s*(.*?)\bbash scripts/release/consumer_smokes/(smoke_\w+\.sh)", re.M)

    def _host_smoke_steps(self) -> list[tuple[str, str]]:
        steps = re.split(r"^      - name: ", self.WORKFLOW.read_text(), flags=re.M)[1:]
        found = []
        for step in steps:
            name = step.splitlines()[0].strip()
            # The Alpine step runs the musl legs only; osx-x64 never reaches it.
            if "matrix.musl == true" in step:
                continue
            if self.INVOKE.search(step) and "matrix.platform" in step:
                found.append((name, step))
        return found

    def test_the_scan_sees_the_host_smokes(self):
        names = [name for name, _ in self._host_smoke_steps()]
        self.assertIn("Smoke cli", names)
        self.assertGreaterEqual(len(names), 8, names)

    def test_every_host_smoke_goes_through_rosetta_on_osx_x64(self):
        for name, step in self._host_smoke_steps():
            with self.subTest(step=name):
                self.assertIn("RUNNER_POOL: ${{ inputs.runner_pool }}", step)
                self.assertIn('"${{ matrix.platform }}" == "osx-x64"', step)
                self.assertIn("rosetta=(bash scripts/release/rosetta_x64.sh)", step)
                for prefix, smoke in self.INVOKE.findall(step):
                    self.assertEqual(prefix, '"${rosetta[@]}" ',
                                     f"{smoke} in '{name}' runs without the Rosetta wrapper")


# Scripts that make a venv or feed a native program: every consumer smoke, and
# the release scripts that build into a throwaway venv. A glob, not a list: a
# new smoke is scanned the day it lands.
def _scanned_scripts() -> list[pathlib.Path]:
    return sorted(SMOKE_DIR.glob("*.sh")) + sorted((REPO / "scripts" / "release").glob("run_*.sh"))


def _code(text: str) -> str:
    """The script with comments blanked, line numbers kept."""
    return "\n".join("" if line.lstrip().startswith("#") else re.sub(r"\s#\s.*$", "", line)
                     for line in text.splitlines())


# A variable whose value is a path, by name. Versions, ids and sentences pass.
_PATHLIKE = re.compile(r"(PATH|DIR|SOURCE|ROOT|FILE|ABS|HOME|TMP|TEMP|WORK|FIXTURE|JAR|NUPKG|WHEEL|TGZ)$|^ARTIFACT$", re.I)
_NATIVE_CFG = re.compile(r"\.(config|csproj|props|targets|xml|toml|npmrc|gradle|kts|properties|json)\b|\bpom\.xml|\.npmrc")
_HEREDOC = re.compile(r"""cat\s+>>?\s*("?[^\s<]+"?)\s*<<-?\s*(['"]?)(\w+)\2""")
_VAR = re.compile(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)")
_COMMAND_POS = r"(?:^|[;&|({]|\bthen\b|\bdo\b|\belse\b)\s*"


def scan_venv_and_native_paths(text: str) -> list[str]:
    """Violations of the two Windows rules, as 'line N: what'.

    1. After a venv exists, nothing goes through the global interpreter:
       no activate, no bare pip/python, and no --quiet/-q on pip (r9d: the
       install's target was invisible).
    2. A path that reaches a native program inside a file or glued to an
       option is converted first (smoke_native_path or cygpath): Git Bash
       rewrites a bare /c/... argument, never one of those (15-10 NU1301).
    """
    code = _code(text)
    lines = code.splitlines()
    found: list[str] = []
    native_vars = {m.group(1) for m in re.finditer(
        r"(?m)\b([A-Za-z_][A-Za-z0-9_]*)=\"?\$\((?:smoke_native_path|cygpath)\b", code)}
    venv_line = next((i for i, l in enumerate(lines) if re.search(r"-m\s+venv\b", l)), None)
    for i, line in enumerate(lines):
        n = i + 1
        if re.search(r"(?:^|[;&|]\s*|\s)(?:source|\.)\s+\S*activate\b", line):
            found.append(f"line {n}: sources a venv activate script")
        if re.search(_COMMAND_POS + r"pip3?\s+install\b", line):
            found.append(f"line {n}: bare pip install (call the venv interpreter by path: \"$VENV_PY\" -m pip)")
        if re.search(r"\bpip3?\b.*\binstall\b", line) and re.search(r"(--quiet|\s-q)\b", line):
            found.append(f"line {n}: pip install --quiet hides where it installs")
        if venv_line is not None and i > venv_line and re.search(_COMMAND_POS + r"python3?\s", line):
            found.append(f"line {n}: bare python after the venv was made (use the venv interpreter by path)")
        for m in re.finditer(r"""(?:\s|^)["']?(-D[\w.]+=|-I|/I|/Fe|-Fe|-L)["']?(\$\{?([A-Za-z_]\w*))""", line):
            name = m.group(3)
            if _PATHLIKE.search(name) and name not in native_vars:
                found.append(f"line {n}: ${name} glued to {m.group(1)} reaches a native program unconverted")
    for m in _HEREDOC.finditer(code):
        target, quoted, tag = m.group(1), m.group(2), m.group(3)
        if quoted or not _NATIVE_CFG.search(target):
            continue
        body_start = code.index("\n", m.end()) + 1
        body_end = code.find(f"\n{tag}\n", body_start)
        body = code[body_start:body_end if body_end >= 0 else len(code)]
        n = code.count("\n", 0, m.start()) + 1
        for v in _VAR.finditer(body):
            name = v.group(1)
            if _PATHLIKE.search(name) and name not in native_vars:
                found.append(f"line {n}: ${name} written into {target} for a native tool without smoke_native_path/cygpath")
    return found


class WindowsVenvAndNativePathScanTests(unittest.TestCase):
    """Every scanned script keeps to the venv rule and the native-path rule.

    r9d win-x64: the 3.11 activate script put a drive-letter path into a
    colon-separated PATH; Git Bash split it, so off that drive pip installed
    into the global interpreter and `import pdfluent` failed in the venv;
    --quiet hid the install's target. 15-10 win-x64: nuget.config carried an
    MSYS path dotnet could not read (NU1301). Both were fixed in one smoke
    each; this scan holds every smoke and release venv script to them.
    """

    def test_the_scan_sees_every_smoke(self):
        names = {p.name for p in _scanned_scripts()}
        for script in SMOKES.values():
            self.assertIn(script.name, names)
        self.assertIn("run_office_tests.sh", names)

    def test_no_script_breaks_either_rule(self):
        for script in _scanned_scripts():
            with self.subTest(script=script.name):
                self.assertEqual(scan_venv_and_native_paths(script.read_text()), [])

    def test_the_python_smokes_check_their_venv(self):
        for name in ("python", "ort"):
            with self.subTest(smoke=name):
                code = _code(SMOKES[name].read_text())
                self.assertIn('"$VENV_PY" -m pip install', code)
                self.assertIn("sys.prefix != sys.base_prefix", code)

    # The negative cases: each line is one the scan must catch, so a scan that
    # goes blind fails here instead of passing everything.
    BAD = {
        "activate": 'python3 -m venv "$W/venv"\nsource "$W/venv/bin/activate"\n',
        "dot-activate": 'python3 -m venv "$W/venv"\n. "$W/venv/Scripts/activate"\n',
        "bare pip": 'python3 -m venv "$W/venv"\npip install pdfluent\n',
        "pip quiet": '"$VENV_PY" -m pip install --quiet pdfluent\n',
        "pip -q": '"$VENV_PY" -m pip install -q pdfluent\n',
        "python after venv": 'python3 -m venv "$W/venv"\npython -m pytest\n',
        "nuget.config": 'cat > "${SMOKE_DIR}/nuget.config" <<XML\n<add key="local" value="${NUGET_SOURCE}" />\nXML\n',
        "Cargo.toml": 'cat >> "${SMOKE_DIR}/Cargo.toml" <<TOML\npdfluent = { path = "${LOCAL_CRATE_PATH_ABS}" }\nTOML\n',
        ".npmrc": 'cat > "$HOME/.npmrc" <<EOF\ncache=$CACHE_DIR\nEOF\n',
        "-Dfile=": '$MVN install:install-file \\\n    -Dfile="${JAR_PATH}" \\\n',
        "cl /I": 'cl /nologo "/I${PKG_DIR}/include" smoke.c\n',
        "cc -I": '"$CC_BIN" -I"$PKG_DIR/include" smoke.c\n',
    }
    GOOD = (
        'python3 -m venv "$W/venv"\nVENV_PY="$W/venv/bin/python"\n"$VENV_PY" -m pip install x\n'
        'NUGET_SOURCE_NATIVE="$(cygpath -w "$NUGET_SOURCE")"\n'
        'cat > "${SMOKE_DIR}/nuget.config" <<XML\n<add value="${NUGET_SOURCE_NATIVE}" v="${EXPECTED_VERSION}"/>\nXML\n'
        "cat > \"${SMOKE_DIR}/smoke.mjs\" <<'JS'\nconst p = '${PKG_DIR}';\nJS\n"
        '$MVN -q exec:java -Dexec.args="\'${SENTENCE}\'"\n'
        '"$CC_BIN" -I"$(smoke_native_path -m "$PKG_DIR/include")" smoke.c\n'
        '# source "$W/venv/bin/activate" is how it went wrong\n'
    )

    def test_the_scan_catches_each_bad_pattern(self):
        for label, text in self.BAD.items():
            with self.subTest(case=label):
                self.assertNotEqual(scan_venv_and_native_paths(text), [], f"scan missed: {label}")

    def test_the_scan_passes_the_fixed_patterns(self):
        self.assertEqual(scan_venv_and_native_paths(self.GOOD), [])

    def test_the_scan_catches_a_bad_fixture_file(self):
        # The file route the real scan takes: a smoke-shaped file on disk.
        with tempfile.TemporaryDirectory() as tmp:
            bad = pathlib.Path(tmp) / "smoke_fixture.sh"
            bad.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + "".join(self.BAD.values()))
            found = scan_venv_and_native_paths(bad.read_text())
            self.assertGreaterEqual(len(found), len(self.BAD), "\n".join(found))


class SmokeDotnetNativeSourcePathTests(unittest.TestCase):
    """The .NET smoke hands dotnet a native path for its local NuGet source (15-10 NU1301 on win-x64)."""

    def test_nuget_config_uses_the_native_path(self):
        text = SMOKES["dotnet"].read_text()
        self.assertIn('value="${NUGET_SOURCE_NATIVE}"', text)
        self.assertNotIn('value="${NUGET_SOURCE}"', text)
        self.assertIn('cygpath -w "$NUGET_SOURCE"', text)


if __name__ == "__main__":
    unittest.main()
