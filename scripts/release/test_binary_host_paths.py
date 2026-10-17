#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The shipped natives name no build host, and the release gates check it.

The 1.1.0 candidates embedded the CI runners' checkout, cargo home and
toolchain paths (209 strings in the linux wheel's extension module). Two
halves hold this:

- binary_host_paths.py over fixtures: a native or wheel that carries a
  runner path fails and names it, a remapped one passes, an archive with
  no native fails instead of passing vacuously, a missing file fails.
- the workflow shape: the release gates run it over the fresh natives
  (node-bindings' Release gate, release-natives' package job, every
  build-wheels.yml leg), and build-wheels.yml -- whose maturin-action
  steps the run-block scan in test_release_rustflags.py cannot see --
  takes release_rustflags.sh before maturin on every leg.
"""
from __future__ import annotations

import io
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "release" / "binary_host_paths.py"
WORKFLOWS = REPO / ".github" / "workflows"

sys.path.insert(0, str(SCRIPT.parent))
import binary_host_paths as bhp  # noqa: E402

# Runner paths, in halves: the snapshot path audit matches whole bytes, so
# the joined fakes look like a real leak while neither half trips it here.
_HOME = "/ho" "me/"
_USERS = "/Use" "rs/"
_VOLUMES = "/Volu" "mes/"
LEAKS = [
    "/srv/r1/_work/engine/engine/crates/pdf-engine/src/lib.rs",
    _HOME + "ghr1/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/zstd-1.0/src/lib.rs",
    _VOLUMES + "BuildDisk/ci/builds/runner-work/x/src/a.rs",
    _USERS + "runner/.rustup/toolchains/1.94.0/lib/rustlib/src/rust/library/core/src/str.rs",
    "C:\\actions-runner-ci\\_work\\engine\\engine\\crates\\pdf-capi\\src\\lib.rs",
    "E:/ns-cargo-home/registry/src/index.crates.io/flate2/src/lib.rs",
]
CLEAN = [
    "/src/crates/pdf-engine/src/lib.rs",
    "/registry/src/index.crates.io-1949cf8c6b5b557f/zstd-1.0/src/lib.rs",
    "/rustc/lib/rustlib/src/rust/library/core/src/str.rs",
    "/buildenv/whatever.rs",
    "called `Option::unwrap()` on a `None` value",
    "application/pdf",
    # Byte patterns from compressed tables in the 1.1.0 dylib that look
    # like a drive letter but carry no directory segment.
    "V:\\yh`u",
    "4W5AQ!#X:/S4cL<F",
    "T:\\b0",
]


def _blob(strings):
    """Bytes shaped like a native: strings separated by non-printable runs."""
    return b"\x7fELF\x02\x01\x01\x00" + b"\x00\x01\x02".join(
        s.encode() for s in strings
    ) + b"\x00"


def _run(*args):
    return subprocess.run(
        [sys.executable, str(SCRIPT), *map(str, args)],
        capture_output=True, text=True,
    )


class ScannerTests(unittest.TestCase):
    def test_every_runner_spelling_is_a_hit(self):
        for leak in LEAKS:
            self.assertEqual(bhp.host_path_strings(_blob([leak])), [leak], leak)

    def test_remap_targets_and_plain_text_are_not(self):
        self.assertEqual(bhp.host_path_strings(_blob(CLEAN)), [])

    def test_clean_native_passes_at_max_zero(self):
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "libpdf_capi.so"
            f.write_bytes(_blob(CLEAN))
            r = _run("--max", "0", f)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn("ok ", r.stdout)

    def test_leaking_native_fails_and_names_the_path(self):
        # The negative fixture: this is the 1.1.0 candidate's shape.
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "index.linux-x64-gnu.node"
            f.write_bytes(_blob(CLEAN + LEAKS))
            r = _run("--max", "0", f)
            self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
            self.assertIn(f"{len(LEAKS)} host-path string(s)", r.stdout)
            self.assertIn("/srv/r1/_work/engine", r.stdout)

    def test_max_is_a_ceiling_not_an_exact_count(self):
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "pdf_capi.dll"
            f.write_bytes(_blob(LEAKS[:2]))
            self.assertEqual(_run("--max", "2", f).returncode, 0)
            self.assertEqual(_run("--max", "1", f).returncode, 1)

    def test_wheel_members_are_read(self):
        with tempfile.TemporaryDirectory() as d:
            whl = pathlib.Path(d) / "pdfluent-1.1.0-cp39-abi3-linux_x86_64.whl"
            with zipfile.ZipFile(whl, "w") as zf:
                zf.writestr("pdfluent/__init__.py", LEAKS[0])  # not a native
                zf.writestr("pdfluent/pdfluent.abi3.so", _blob(LEAKS[:1]))
            r = _run("--max", "0", whl)
            self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
            self.assertIn("pdfluent.abi3.so", r.stdout)
            self.assertNotIn("__init__.py", r.stdout)

    def test_tarball_members_are_read(self):
        with tempfile.TemporaryDirectory() as d:
            tgz = pathlib.Path(d) / "node-linux-x64-gnu-1.1.0.tgz"
            data = _blob(CLEAN)
            with tarfile.open(tgz, "w:gz") as tf:
                info = tarfile.TarInfo("package/pdfluent.linux-x64-gnu.node")
                info.size = len(data)
                tf.addfile(info, io.BytesIO(data))
            r = _run("--max", "0", tgz)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_archive_without_a_native_fails_not_passes(self):
        with tempfile.TemporaryDirectory() as d:
            whl = pathlib.Path(d) / "empty.whl"
            with zipfile.ZipFile(whl, "w") as zf:
                zf.writestr("pdfluent/__init__.py", "")
            r = _run("--max", "0", whl)
            self.assertEqual(r.returncode, 1)
            self.assertIn("holds no native", r.stderr)

    def test_missing_file_fails(self):
        r = _run("--max", "0", "/nonexistent/libpdf_capi.so")
        self.assertEqual(r.returncode, 1)
        self.assertIn("no such file", r.stderr)



# One positive per host-path spelling the runners produce, one negative per
# form that must stay quiet. Raw strings: the bytes as they sit in a binary.
VARIANT_HITS = {
    "mac volume": _VOLUMES + "BuildDisk/ci/builds/w/crates/a.rs",
    "mac home": _USERS + "runner/.cargo/registry/src/x/lib.rs",
    "linux home": _HOME + "ghr1/.cargo/registry/src/x/lib.rs",
    "linux root": "/root/.cargo/registry/src/x/lib.rs",
    "srv runner": "/srv/r1/_work/engine/engine/crates/a.rs",
    "srv other": "/srv/ci/_work/engine/crates/a.rs",
    "tmp runner": "/tmp/github-runner-7/_work/engine/crates/a.rs",
    "private tmp runner": "/private" "/tmp/actions-runner/_work/a.rs",
    "mac temp": "/private/var" "/folders/bc/xyz/T/cargo/a.rs",
    "C forward": "C:/actions-runner-ci/_work/engine/a.rs",
    "C back": r"C:\actions-runner-ci\_work\engine\a.rs",
    "C escaped": r"C:\\actions-runner-ci\\_work\\engine\\a.rs",
    "D back": r"D:\a\engine\engine\crates\a.rs",
    "E forward": "E:/runner-work-win/engine/a.rs",
    "E lower": r"e:\ns-cargo-home\registry\src\a.rs",
    "msys c": "/c/actions-runner-ci/_work/engine/a.rs",
    "msys e": "/e/ns-cargo-home/registry/src/a.rs",
    "unc long": r"\\?\E:\runner-work-win\engine\a.rs",
    "unc device": r"\\.\pipe\x",
    "unc forward": "//?/C:/x",
    "file url": "file://" + _HOME + "ghr1/.cargo/registry/a.rs",
    "hostedtoolcache": "/opt/hostedtoolcache/Python/3.11.9/x64/include/python3.11/a.h",
    "github workspace": "/github/workspace/crates/pdf-engine/src/lib.rs",
    "container __w": "/__w/engine/engine/crates/pdf-engine/src/lib.rs",
    "build root": "/build/engine/crates/a.rs",
    "workspace root": "/workspace/crates/a.rs",
    "maturin io": "/io/crates/pdf-python/src/lib.rs",
    "mnt": "/mnt/data/runner/_work/a.rs",
    "docker cargo": "/usr/local/cargo/registry/src/index.crates.io/a.rs",
    "docker rustup": "/usr/local/rustup/toolchains/1.94.0/lib/rustlib/src/a.rs",
    "manylinux python": "/opt/python/cp311-cp311/include/python3.11/a.h",
    "any tmp": "/tmp/cargo-installXyz/release/build/a.rs",
    "mac private tmp": "/private" "/tmp/xfa/target/a.rs",
    "msys short": "/e/a/b.rs",
    "networkservice home": r"C:\WINDOWS\ServiceProfiles\NetworkService\.cargo\registry\a.rs",
    "unc verbatim unc": r"\\?\UNC\fileserver\share\a.rs",
    "msvc quickjs file": r"C:\actions-runner-ci\_work\engine\engine\target\release\build\rquickjs-sys-7177a966f0570837\out\quickjs.c",
    "below windows fonts": r"C:\Windows\Fonts\..\..\actions-runner\_work\a.rs",
    "glued drive": r"attempting to repairC:\actions-runner-ci\_work\engine\a.rs",
    "glued cargo home": r"iterationE:\ns-cargo-home\registry\src\a.rs",
    "run together": "/lenIV" + _VOLUMES + "BuildDisk/w/crates/pdf-manip",
}
VARIANT_QUIET = {
    "src": "/src/crates/pdf-engine/src/lib.rs",
    "cargo": "/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/a.rs",
    "rustup": "/rustup/toolchains/1.94.0/lib/rustlib/src/rust/library/core/src/a.rs",
    "rustc": "/rustc/4a4ef493e/library/core/src/str/mod.rs",
    "buildenv": "/buildenv/.cargo/registry/a.rs",
    "git": "/git/checkouts/hayro-1/abc/src/a.rs",
    "registry": "/registry/src/index.crates.io/a.rs",
    "repo relative": "crates/pdf-engine/src/lib.rs",
    "https": "https://github.com/pdfluent/engine/blob/main/c/x/y.rs",
    "https home": "https://example.com" + _HOME + "user/doc",
    "http msys-like": "http://host/c/foo/bar",
    "system": "/usr/share/poppler/cMap",
    "drive no segment": r"V:\yh`u",
    "ratio": "1:/2 and 3:\\4",
    "devtoolset headers": "/opt/rh/devtoolset-10/root/usr/include/c++/10/bits/stl_vector.h",
    "rustc hash": "/rustc/4a4ef493e2c3e3bb0ef0b44a0ebbd8ae4e7ca2b5/library/std/src/panicking.rs",
    "relative build dir": "crates/pdf-engine/build/out.rs",
    "relative io": "std/io/error.rs",
    "short noise": "a/b/c",
    "sed-like": "s/a/b/",
    "char table": "/&/,/2/8/>/D/J/P/V/\\/b/h/n/t/z/",
    "char table 2": "/{/w/t/p/l/i/e/a/^/Z/V/R/O/K/H/D/@/=/9/5/2/./+/'/#/ /",
    "https msys-like": "https://github.com/e/a/b.rs",
    # Constants of Rust's std in every Windows binary (1.1.0 win-x64, 15-10).
    "verbatim prefix": "\\\\?\\",
    "verbatim unc prefix": "\\\\?\\\\\\?\\UNC\\",
    "device nul": "exe\\cmd.exe\\\\.\\NULstack backtrace:",
    # The system font directory pdfa_fonts.rs reads, run together with the
    # next constant in the string table.
    "windows fonts": "C:\\Windows\\Fonts\\HOME",
    # Byte noise in the macOS C API dylib (16-10) that a plain-UNC rule read as a
    # path; plain UNC is the end scan's job, so it stays quiet here.
    "dylib noise": "\\\\\\ll\\lX\\t0\\",
    # Escape tables and machine-code runs that look like UNC (scan2, 15-10).
    "escape table": "\\\\n\\r\\t",
    "machine code run": "\\\\XPLTLT\\XHDD4\\P",
}


class KnownConstantTests(unittest.TestCase):
    """The phf hash key in front of a remapped path is not a drive (16-10)."""

    KEY = bytes.fromhex("4e284e8ee4cd553a")
    PATH = b"/src\\crates\\pdf-interpret\\src\\shading.rs"

    def test_the_exact_constant_before_a_remapped_path_is_quiet(self):
        data = b"\x00" * 8 + self.KEY + self.PATH + b"\x00"
        self.assertEqual(bhp.host_path_strings(data), [])

    def test_one_changed_byte_of_the_constant_is_still_a_hit(self):
        key = bytes.fromhex("4e284e8ee4ce553a")
        data = b"\x00" * 8 + key + self.PATH + b"\x00"
        self.assertEqual(len(bhp.host_path_strings(data)), 1)

    def test_a_real_drive_before_a_remap_target_is_still_a_hit(self):
        for text in (b"U:/src\\crates\\a.rs", b"\xcdU:/src\\crates\\a.rs", b"xU:/src/crates/a.rs"):
            with self.subTest(text=text):
                self.assertEqual(len(bhp.host_path_strings(b"\x00" + text + b"\x00")), 1)

    def test_host_paths_next_to_the_constant_are_still_hits(self):
        for text in (b"U:\\actions-runner\\_work\\a.rs", b"HOMEC:\\actions-runner\\_work\\a.rs"):
            with self.subTest(text=text):
                data = b"\x00" + self.KEY + b"\x00" + text + b"\x00"
                self.assertEqual(len(bhp.host_path_strings(data)), 1)


class VariantTests(unittest.TestCase):
    def test_every_host_spelling_is_a_hit(self):
        for label, text in VARIANT_HITS.items():
            with self.subTest(label):
                self.assertEqual(bhp.host_path_strings(_blob([text])), [text])

    def test_every_remapped_relative_or_web_form_is_quiet(self):
        for label, text in VARIANT_QUIET.items():
            with self.subTest(label):
                self.assertEqual(bhp.host_path_strings(_blob([text])), [])

    def test_utf16le_strings_are_read_too(self):
        # PE files store many paths as UTF-16LE; an ASCII-only scan saw none.
        leak = r"C:\actions-runner-ci\_work\engine\a.rs"
        data = b"MZ\x90\x00" + leak.encode("utf-16-le") + b"\x00\x00"
        self.assertEqual(bhp.host_path_strings(data), [leak])
        clean = "/src/crates/a.rs".encode("utf-16-le")
        self.assertEqual(bhp.host_path_strings(b"MZ\x00\x00" + clean + b"\x00\x00"), [])

    def test_a_url_does_not_hide_a_path_beside_it(self):
        text = "see https://pdfluent.com/docs at /srv/r1/_work/a.rs"
        self.assertEqual(bhp.host_path_strings(_blob([text])), [text])



class ReleaseCfgTests(unittest.TestCase):
    """`env!` paths are out of --remap-path-prefix's reach: release builds set
    `pdfluent_release` and the checkout-only lookups drop out under it."""

    SITES = {
        "crates/pdf-manip/src/pdfa_fonts.rs": 2,  # CMap dir + font-pack dir
        "crates/pdf-manip/src/pdfa_fixups.rs": 1,  # CMap dir
    }

    def test_the_helper_sets_the_release_cfg(self):
        env = {k: v for k, v in __import__("os").environ.items() if not k.startswith("GIT_")}
        env.pop("RUSTFLAGS", None)
        out = subprocess.run(
            ["bash", "-c", 'source scripts/release/release_rustflags.sh && printf %s "$RUSTFLAGS"'],
            capture_output=True, text=True, env=env, cwd=str(REPO),
        ).stdout
        self.assertIn("--cfg pdfluent_release", out)

    def test_every_checkout_only_lookup_drops_out_in_release(self):
        for rel, want in self.SITES.items():
            lines = (REPO / rel).read_text().splitlines()
            gated = 0
            for i, line in enumerate(lines):
                if 'env!("CARGO_MANIFEST_DIR")' in line and "#[test]" not in "".join(lines[max(0, i - 3):i]):
                    window = "\n".join(lines[max(0, i - 6):i])
                    if "not(pdfluent_release)" in window:
                        gated += 1
            with self.subTest(rel):
                self.assertGreaterEqual(gated, want)
        toml = (REPO / "crates/pdf-manip/Cargo.toml").read_text()
        self.assertIn('check-cfg = ["cfg(pdfluent_release)"]', toml)

class CheckScriptTests(unittest.TestCase):
    """check_build_host_paths.sh: prints the block, then judges the files."""

    def _run(self, *files):
        env = {k: v for k, v in __import__("os").environ.items() if not k.startswith("GIT_")}
        env.pop("RUSTFLAGS", None)
        return subprocess.run(
            ["bash", str(REPO / CHECK), *map(str, files)],
            capture_output=True, text=True, env=env, cwd=str(REPO),
        )

    def test_prints_the_block_and_passes_a_clean_native(self):
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "libpdf_capi.so"
            f.write_bytes(_blob(CLEAN))
            r = self._run(f)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn("Remap block on this runner", r.stdout)
            self.assertRegex(r.stdout, r"-> /src  \(exists")

    def test_fails_a_leaking_native(self):
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "pdfluent.abi3.so"
            f.write_bytes(_blob(LEAKS))
            r = self._run(f)
            self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
            self.assertIn("/srv/r1/_work/engine", r.stdout)

    def test_no_files_is_a_usage_error_not_a_pass(self):
        self.assertEqual(self._run().returncode, 2)

def _load(name):
    return yaml.safe_load((WORKFLOWS / name).read_text())


def _steps(doc, job):
    return [s for s in doc["jobs"][job].get("steps") or [] if isinstance(s, dict)]


def _code(text):
    return "\n".join(
        l for l in (text or "").splitlines() if not l.strip().startswith("#")
    )


def _gate_index(steps):
    """Index of the step that runs binary_host_paths.py --max 0, or -1."""
    for i, s in enumerate(steps):
        code = _code(s.get("run"))
        if "binary_host_paths.py" in code and "--max 0" in code:
            return i
    return -1


SOURCES_HELPER = "scripts/release/release_rustflags.sh"
CHECK = "scripts/release/check_build_host_paths.sh"

sys.path.insert(0, str(REPO / "scripts" / "release"))
from test_release_rustflags import EXPECTED_BUILD_STEPS  # noqa: E402

# Registered build steps that ship no native of their own, with the reason.
NO_NATIVE_TO_CHECK = {
    ("wasm.yml", "wasm-build", "Build wasm package"): "a wasm32 module, no host native",
    ("release-binaries.yml", "create-release", "Build host binary for the release gate"):
        "the gate's smoke binary; the shipped CLIs are checked in the build job",
}


class WorkflowShapeTests(unittest.TestCase):
    def test_node_release_gate_checks_the_fresh_bindings(self):
        steps = _steps(_load("node-bindings.yml"), "release-gate")
        i = _gate_index(steps)
        self.assertGreaterEqual(i, 0, "node-bindings release-gate never runs binary_host_paths.py --max 0")
        self.assertIn("*.node", steps[i]["run"])
        self.assertNotIn("continue-on-error", steps[i])

    def test_natives_package_checks_every_native_and_wheel(self):
        steps = _steps(_load("release-natives.yml"), "package")
        i = _gate_index(steps)
        self.assertGreaterEqual(i, 0, "release-natives package never runs binary_host_paths.py --max 0")
        run = steps[i]["run"]
        for pattern in ("*.so", "*.dylib", "*.dll", "*.whl"):
            self.assertIn(pattern, run)
        self.assertNotIn("continue-on-error", steps[i])
        flatten = next(
            j for j, s in enumerate(steps) if s.get("name") == "Flatten the natives layout"
        )
        self.assertGreater(i, flatten, "the check must read the downloaded natives")

    def test_every_release_build_step_is_followed_by_the_host_path_check(self):
        # Minutes in, not hours later: the step right after each build runs
        # the check over that job's own fresh artefacts.
        checked = 0
        for wf, entries in EXPECTED_BUILD_STEPS.items():
            doc = _load(wf)
            for job, step_name in entries:
                if (wf, job, step_name) in NO_NATIVE_TO_CHECK:
                    continue
                steps = _steps(doc, job)
                i = next(j for j, s in enumerate(steps) if s.get("name") == step_name)
                with self.subTest(f"{wf} {job} {step_name}"):
                    self.assertLess(i + 1, len(steps))
                    nxt = _code(steps[i + 1].get("run"))
                    self.assertIn(CHECK, nxt, "the step after the build is not the host-path check")
                    self.assertNotIn("--remaps-only", nxt)
                    self.assertEqual(steps[i + 1].get("shell"), "bash")
                    self.assertNotIn("continue-on-error", steps[i + 1])
                checked += 1
        self.assertGreaterEqual(checked, 4)

    def test_exemptions_name_registered_steps(self):
        registered = {
            (wf, job, step) for wf, e in EXPECTED_BUILD_STEPS.items() for job, step in e
        }
        self.assertLessEqual(set(NO_NATIVE_TO_CHECK), registered)

    def test_every_build_wheels_leg_scrubs_before_maturin_and_checks_after(self):
        doc = _load("build-wheels.yml")
        for job in ("linux", "macos", "windows"):
            steps = _steps(doc, job)
            build = next(
                j for j, s in enumerate(steps)
                if str(s.get("uses", "")).startswith("PyO3/maturin-action")
            )
            maturin = steps[build]
            if job == "linux":
                # Linux builds inside the manylinux container: the helper must
                # run there, from the before-script, so its paths are the
                # container's own.
                self.assertIn(
                    SOURCES_HELPER.split("/", 2)[-1],
                    _code(maturin["with"].get("before-script-linux")),
                    "linux leg: before-script-linux does not source the helper",
                )
            else:
                scrub = [
                    s for s in steps[:build]
                    if SOURCES_HELPER in _code(s.get("run"))
                    and "GITHUB_ENV" in _code(s.get("run"))
                ]
                self.assertTrue(scrub, f"{job} leg: no step exports the remap block before maturin")
                self.assertEqual(scrub[-1].get("shell"), "bash")
                # The C prefix map rides in CFLAGS/CXXFLAGS: a step that hands
                # maturin only RUSTFLAGS compiles C with the builder's paths.
                export = _code(scrub[-1].get("run"))
                for var in ("RUSTFLAGS", "C(XX)?FLAGS", "|CL)"):
                    self.assertIn(var, export, f"{job} leg: the export step does not hand {var} to maturin")
            after = steps[build + 1]
            self.assertIn(CHECK, _code(after.get("run")), f"{job} leg: the step after maturin is not the host-path check")
            self.assertIn("*.whl", after["run"])
            self.assertNotIn("--remaps-only", _code(after["run"]))


# Every shipped cdylib and the install name its build script pins on Apple
# targets: ld64 otherwise stamps LC_ID_DYLIB with the builder's target dir,
# which --remap-path-prefix never reaches.
CDYLIB_INSTALL_NAMES = {
    "pdf-capi": "@rpath/libpdf_capi.dylib",
    "pdf-java": "@rpath/libpdfluent_java.dylib",
    "pdf-node": "@rpath/libpdf_node.dylib",
}


class InstallNameTests(unittest.TestCase):
    def test_every_shipped_cdylib_pins_its_apple_install_name(self):
        for crate, name in CDYLIB_INSTALL_NAMES.items():
            build = REPO / "crates" / crate / "build.rs"
            self.assertTrue(build.is_file(), f"{crate} has no build.rs")
            code = build.read_text()
            self.assertIn(
                f"cargo:rustc-cdylib-link-arg=-Wl,-install_name,{name}", code, crate
            )
            self.assertIn('"apple"', code, f"{crate}: the arg must be Apple-only")

    def test_every_cdylib_crate_is_registered_or_excused(self):
        # A new cdylib crate must pin its install name or say why it ships
        # no Apple dylib.
        cdylibs = {
            toml.parent.name
            for toml in (REPO / "crates").glob("*/Cargo.toml")
            if '"cdylib"' in toml.read_text()
        }
        self.assertEqual(
            sorted(cdylibs - set(NOT_A_SHIPPED_APPLE_DYLIB)),
            sorted(CDYLIB_INSTALL_NAMES),
        )


NOT_A_SHIPPED_APPLE_DYLIB = {
    # The wasm32 module: no Mach-O, no install name.
    "xfa-wasm": "wasm only",
    # Consumed as an rlib by the bindings; its own cdylib is never shipped.
    "pdf-engine": "rlib consumer only",
    # maturin already links the extension as @rpath/pdfluent._native.abi3.so
    # (measured 15-10-2026); a build script would override that name.
    "pdf-python": "maturin pins the install name",
}

if __name__ == "__main__":
    unittest.main(verbosity=2)
