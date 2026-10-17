#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Release builds scrub the builder's own paths, on any runner (#636).

`.cargo/config.toml` names one Mac's home in its --remap-path-prefix block,
so every artefact built anywhere else embedded the runner's paths: the Linux
wheels failed fix-wheel, and the other channels were never checked. Every
release build now sources `scripts/release/release_rustflags.sh` first, which
computes the same mapping from the build's actual locations — the home, the
repo root, the cargo home's git and registry trees, and rustc's sysroot — in
last-match-wins order, plus the per-target extras env RUSTFLAGS must repeat
because they replace the config outright.

These cases source the helper in fake Linux, Windows and Mac builder
environments (stub rustc, scrubbed env, no toolchain needed) and hold the
remaps, their order and the extras; and they hold the workflow shape, that
every artefact-producing step in the four release workflows sources the
helper on bash before it builds, with the musl cdylib steps keeping the musl
wrapper and the musl CLI never taking it.
"""
from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
HELPER = REPO / "scripts" / "release" / "release_rustflags.sh"
MUSL_HELPER = REPO / "scripts" / "release" / "musl_cdylib_rustflags.sh"
WORKFLOWS = REPO / ".github" / "workflows"

# Machine paths, in halves: the snapshot path audit only matches whole bytes,
# so the joined fakes exercise the helper exactly like a runner's paths while
# neither half trips the audit on this file.
_HOME = "/home/"
_USERS = "/Users/"

_UNSET = object()  # remove the variable from the child's environment


def _git_env():
    """os.environ without any GIT_*: the pre-push hook exports GIT_DIR, and
    a git answer read through it would be fooled the same way as the bug."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def _sourced_rustflags(
    *,
    home=_UNSET,
    workspace=_UNSET,
    cargo_home=_UNSET,
    userprofile=_UNSET,
    sysroot=_UNSET,
    rustflags=_UNSET,
    git_dir=_UNSET,
    git_ok=True,
    git_dubious=False,
    args=(),
    helper=HELPER,
    cwd=None,
    extra_bins=None,
    extra_env=None,
    print_var="RUSTFLAGS",
):
    """Source the helper in a scrubbed fake-builder environment.

    Returns (returncode, RUSTFLAGS, stderr). `sysroot` is what the stub rustc
    answers to --print sysroot; _UNSET stubs a rustc that fails. `git_ok`
    False stubs a git that fails; `git_dubious` stubs one that refuses with
    the dubious-ownership error and exit 128, as git does for a checkout
    owned by another user when the scrubbed env carries no safe.directory
    config. `git_dir` exports GIT_DIR naming that git dir, as the pre-push
    hook does; without it no GIT_* variable is inherited. Everything else
    is the exact environment the case wants, with no inherited builder paths.
    """
    with tempfile.TemporaryDirectory(prefix="rrf_") as tmp:
        bindir = pathlib.Path(tmp) / "bin"
        bindir.mkdir()
        rustc = bindir / "rustc"
        if sysroot is _UNSET:
            rustc.write_text("#!/bin/sh\nexit 1\n")
        else:
            rustc.write_text(f"#!/bin/sh\nprintf '%s\\n' '{sysroot}'\n")
        rustc.chmod(0o755)
        for name, body in (extra_bins or {}).items():
            stub = bindir / name
            stub.write_text(body)
            stub.chmod(0o755)
        if git_dubious:
            git = bindir / "git"
            git.write_text(
                "#!/bin/sh\n"
                'echo "fatal: detected dubious ownership in repository" >&2\n'
                "exit 128\n"
            )
            git.chmod(0o755)
        elif not git_ok:
            git = bindir / "git"
            git.write_text("#!/bin/sh\nexit 1\n")
            git.chmod(0o755)
        env = dict(os.environ)
        for var in (
            "HOME",
            "GITHUB_WORKSPACE",
            "CARGO_HOME",
            "USERPROFILE",
            "RUSTUP_HOME",
            "RUSTFLAGS",
        ):
            env.pop(var, None)
        for var in [v for v in env if v.startswith("GIT_")]:
            env.pop(var, None)
        if git_dir is not _UNSET:
            env["GIT_DIR"] = git_dir
        if home is not _UNSET:
            env["HOME"] = home
        if workspace is not _UNSET:
            env["GITHUB_WORKSPACE"] = workspace
        if cargo_home is not _UNSET:
            env["CARGO_HOME"] = cargo_home
        if userprofile is not _UNSET:
            env["USERPROFILE"] = userprofile
        if rustflags is not _UNSET:
            env["RUSTFLAGS"] = rustflags
        for var in [v for v in env if "FLAGS" in v and v.startswith(("CFLAGS", "CXXFLAGS", "TARGET_C", "HOST_C"))]:
            env.pop(var, None)
        env.update(extra_env or {})
        env["PATH"] = f"{bindir}{os.pathsep}{env.get('PATH', '')}"
        cmd = f"source {helper}"
        if args:
            cmd += " " + " ".join(args)
        cmd += f' && printf "%s" "${print_var}"'
        proc = subprocess.run(
            ["bash", "-c", cmd],
            capture_output=True,
            text=True,
            timeout=30,
            env=env,
            cwd=str(cwd or REPO),
        )
        return proc.returncode, proc.stdout, proc.stderr


def _assert_ordered(test, flags, targets=("=/buildenv", "=/src", "=/git", "=/registry", "=/rustc")):
    """The targets appear in last-match-wins order: catch-all first."""
    idx = [flags.index(t) for t in targets]
    test.assertEqual(idx, sorted(idx), f"last-match-wins order broken in: {flags}")


def _src_of(test, flags, target="=/src"):
    """The source path the helper mapped to `target` (first spelling)."""
    for token in flags.split():
        if token.endswith(target):
            return token[: -len(target)]
    raise test.failureException(f"no {target} rule in: {flags}")


class LinuxRunnerTests(unittest.TestCase):
    """A GitHub-hosted Linux builder: everything under one Linux home."""

    LINUX_HOME = _HOME + "runner"
    LINUX_WS = _HOME + "runner/work/engine/engine"
    LINUX_CARGO = _HOME + "runner/.cargo"
    LINUX_SYSROOT = _HOME + "runner/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu"

    def test_linux_runner_paths_all_remap_in_order(self):
        rc, flags, err = _sourced_rustflags(
            home=self.LINUX_HOME,
            workspace=self.LINUX_WS,
            cargo_home=self.LINUX_CARGO,
            sysroot=self.LINUX_SYSROOT,
        )
        self.assertEqual(rc, 0, err)
        for rule in (
            f"{self.LINUX_HOME}=/buildenv",
            f"{self.LINUX_WS}=/src",
            f"{self.LINUX_CARGO}/git=/git",
            f"{self.LINUX_CARGO}/registry=/registry",
            f"{self.LINUX_SYSROOT}=/rustc",
        ):
            self.assertIn(rule, flags)
        _assert_ordered(self, flags)
        self.assertNotIn("crt-static", flags)
        self.assertNotIn("simd128", flags)

    def test_workspace_wins_over_the_real_checkout(self):
        # This runs inside the real checkout with a working git: the fake
        # workspace must still be the one mapped, and the real toplevel must
        # appear nowhere.
        toplevel = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=str(REPO),
            env=_git_env(),
        ).stdout.strip()
        self.assertTrue(toplevel)
        _, flags, _ = _sourced_rustflags(
            home=self.LINUX_HOME,
            workspace=self.LINUX_WS,
            cargo_home=self.LINUX_CARGO,
            sysroot=self.LINUX_SYSROOT,
        )
        self.assertIn(f"{self.LINUX_WS}=/src", flags)
        self.assertNotIn(toplevel, flags)


class MacRunnerTests(unittest.TestCase):
    """The Mac the config block was written for, derived instead of pinned."""

    MAC_HOME = _USERS + "dev"
    MAC_WS = _USERS + "dev/Documents/XFA"
    MAC_SYSROOT = _USERS + "dev/.rustup/toolchains/1.94.0-aarch64-apple-darwin"

    def test_mac_paths_reproduce_the_config_block_dynamically(self):
        # CARGO_HOME unset, as on a plain Mac: the default $HOME/.cargo feeds
        # the git and registry rules, and the old checkout path maps to /src.
        rc, flags, err = _sourced_rustflags(
            home=self.MAC_HOME,
            workspace=self.MAC_WS,
            sysroot=self.MAC_SYSROOT,
        )
        self.assertEqual(rc, 0, err)
        for rule in (
            f"{self.MAC_HOME}=/buildenv",
            f"{self.MAC_WS}=/src",
            f"{self.MAC_HOME}/.cargo/git=/git",
            f"{self.MAC_HOME}/.cargo/registry=/registry",
            f"{self.MAC_SYSROOT}=/rustc",
        ):
            self.assertIn(rule, flags)
        _assert_ordered(self, flags)
        self.assertNotIn("crt-static", flags)
        self.assertNotIn("simd128", flags)


class WindowsRunnerTests(unittest.TestCase):
    """A Windows builder under Git Bash: Unix and drive-letter spellings map."""

    WIN_HOME_UNIX = "/c" + _USERS + "runneradmin"
    WIN_HOME_BACK = "C:\\Users\\runneradmin"
    WIN_HOME_FWD = "C:" + _USERS + "runneradmin"
    WIN_WS = "C:\\actions-runner\\_work\\engine\\engine"
    WIN_CARGO = "C:\\Users\\runneradmin\\.cargo"
    WIN_SYSROOT = "C:\\Users\\runneradmin\\.rustup\\toolchains\\1.94.0-x86_64-pc-windows-msvc"

    def _flags(self):
        rc, flags, err = _sourced_rustflags(
            home=self.WIN_HOME_UNIX,
            userprofile=self.WIN_HOME_BACK,
            workspace=self.WIN_WS,
            cargo_home=self.WIN_CARGO,
            sysroot=self.WIN_SYSROOT,
        )
        self.assertEqual(rc, 0, err)
        return flags

    def test_msvc_trims_every_backslash_drive_path_from_file(self):
        # 15-10: pdf_capi.dll named C:\actions-runner-pdfluent\_work\...\quickjs.c
        # through __FILE__; cl takes -d1trimfile:<prefix>\ instead of a map.
        cyg = {"cygpath": "#!/bin/sh\nprintf '%s\\n' \"$2\"\n"}
        rc, cflags, err = _sourced_rustflags(
            home=self.WIN_HOME_UNIX, userprofile=self.WIN_HOME_BACK, workspace=self.WIN_WS,
            cargo_home=self.WIN_CARGO, sysroot=self.WIN_SYSROOT, extra_bins=cyg, print_var="CFLAGS",
        )
        self.assertEqual(rc, 0, err)
        for prefix in (self.WIN_WS, self.WIN_HOME_BACK, self.WIN_CARGO + "\\registry", self.WIN_SYSROOT):
            self.assertIn(f"-d1trimfile:{prefix}\\", cflags.split(" "))
        for flag in cflags.split(" "):
            self.assertTrue(flag.startswith("-d1trimfile:") and flag[12:13].isalpha() and flag[13:15] == ":\\", flag)
        self.assertNotIn("prefix-map", cflags)
        self.assertEqual(cflags.count(f"-d1trimfile:{self.WIN_WS}\\"), 1)

    def test_msvc_trim_also_rides_cl_which_build_scripts_cannot_replace(self):
        # rquickjs-sys sets CFLAGS outright on MSVC; cl reads CL on every call.
        cyg = {"cygpath": "#!/bin/sh\nprintf '%s\\n' \"$2\"\n"}
        rc, cl, err = _sourced_rustflags(
            home=self.WIN_HOME_UNIX, userprofile=self.WIN_HOME_BACK, workspace=self.WIN_WS,
            cargo_home=self.WIN_CARGO, sysroot=self.WIN_SYSROOT, extra_bins=cyg, print_var="CL",
            extra_env={"CL": "/MP"},
        )
        self.assertEqual(rc, 0, err)
        self.assertTrue(cl.startswith("/MP "), cl)
        self.assertIn(f"-d1trimfile:{self.WIN_WS}\\", cl.split(" "))
        self.assertEqual(cl.count(f"-d1trimfile:{self.WIN_WS}\\"), 1)
        # Sourcing twice (a step that sources the helper again) adds it once.
        rc, cl2, err = _sourced_rustflags(
            home=self.WIN_HOME_UNIX, userprofile=self.WIN_HOME_BACK, workspace=self.WIN_WS,
            cargo_home=self.WIN_CARGO, sysroot=self.WIN_SYSROOT, extra_bins=cyg, print_var="CL",
            args=("", "&&", "source", str(HELPER)),
        )
        self.assertEqual(rc, 0, err)
        self.assertEqual(cl2.count(f"-d1trimfile:{self.WIN_WS}\\"), 1, cl2)

    def test_home_maps_in_every_spelling(self):
        flags = self._flags()
        for spelling in (self.WIN_HOME_UNIX, self.WIN_HOME_FWD, self.WIN_HOME_BACK):
            self.assertIn(f"{spelling}=/buildenv", flags)

    def test_workspace_and_cargo_map_in_every_spelling(self):
        flags = self._flags()
        ws_unix = "/c/actions-runner/_work/engine/engine"
        ws_fwd = "C:/actions-runner/_work/engine/engine"
        for spelling in (self.WIN_WS, ws_fwd, ws_unix):
            self.assertIn(f"{spelling}=/src", flags)
        cargo_unix = "/c" + _USERS + "runneradmin/.cargo"
        for leaf in ("git", "registry"):
            self.assertIn(f"{cargo_unix}/{leaf}=/{leaf}", flags)
            self.assertIn(f"{self.WIN_CARGO}\\{leaf}=/{leaf}", flags)

    def test_sysroot_maps_and_order_holds(self):
        flags = self._flags()
        sysroot_unix = "/c" + _USERS + "runneradmin/.rustup/toolchains/1.94.0-x86_64-pc-windows-msvc"
        self.assertIn(f"{self.WIN_SYSROOT}=/rustc", flags)
        self.assertIn(f"{sysroot_unix}=/rustc", flags)
        _assert_ordered(self, flags)
        self.assertNotIn("crt-static", flags)
        self.assertNotIn("simd128", flags)


class SymlinkedPathTests(unittest.TestCase):
    """A symlinked cargo home or checkout maps under BOTH spellings.

    The Mac runner's ~/.cargo/registry is a symlink onto the build volume
    and the Windows runner's workspace and cargo home are junctions onto
    E:; rustc records whichever spelling the path reached it by, so the
    helper maps the real location beside the logical one, same target,
    same order. (The junction route needs Windows; this holds the symlink
    route, which is the same code path up to the resolver.)
    """

    def test_real_location_maps_beside_the_symlink(self):
        with tempfile.TemporaryDirectory(prefix="rrf_link_") as tmp:
            root = pathlib.Path(tmp).resolve()
            real_cargo = root / "volume" / "cargo-home"
            (real_cargo / "registry").mkdir(parents=True)
            (real_cargo / "git").mkdir()
            real_ws = root / "volume" / "work" / "engine"
            real_ws.mkdir(parents=True)
            home = root / "home"
            home.mkdir()
            (home / ".cargo").symlink_to(real_cargo, target_is_directory=True)
            ws_link = root / "ws"
            ws_link.symlink_to(real_ws, target_is_directory=True)
            rc, flags, err = _sourced_rustflags(
                home=str(home), workspace=str(ws_link), sysroot=_UNSET,
            )
            self.assertEqual(rc, 0, err)
            for src, tgt in (
                (home / ".cargo" / "registry", "/registry"),
                (real_cargo / "registry", "/registry"),
                (home / ".cargo" / "git", "/git"),
                (real_cargo / "git", "/git"),
                (ws_link, "/src"),
                (real_ws, "/src"),
            ):
                self.assertIn(f"--remap-path-prefix {src}={tgt}", flags)
            _assert_ordered(self, flags, ("=/buildenv", "=/src", "=/git", "=/registry"))

    def test_windows_junction_on_an_ancestor_maps_the_real_path(self):
        # The Windows runner: the workspace C:\...\_work\engine\engine sits
        # UNDER the junction C:\...\_work -> E:\runner-work-win, which Git
        # Bash's pwd -P cannot see. The stub powershell answers for that
        # ancestor ONLY, as Windows does: asking the leaf alone finds nothing.
        with tempfile.TemporaryDirectory(prefix="rrf_junc_") as tmp:
            work = pathlib.Path(tmp).resolve() / "_work"
            ws = work / "engine" / "engine"
            ws.mkdir(parents=True)
            win = lambda p: str(p).replace("/", "\\")
            stubs = {
                # cygpath -w: the same path, backslashed.
                "cygpath": "#!/bin/sh\nprintf '%s\\n' \"$2\" | tr / '\\\\\\\\'\n",
                "powershell.exe": (
                    "#!/bin/sh\n"
                    f"case \"$*\" in *\"'{win(work)}'\"*) "
                    f"printf '%s\\r\\n' '{win(work)}|E:\\runner-work-win' ;; esac\n"
                ),
            }
            rc, flags, err = _sourced_rustflags(
                home=str(work.parent), workspace=str(ws), sysroot=_UNSET,
                extra_bins=stubs,
            )
            self.assertEqual(rc, 0, err)
            self.assertIn(f"--remap-path-prefix {ws}=/src", flags)
            for spelling in (
                r"E:\runner-work-win\engine\engine=/src",
                "E:/runner-work-win/engine/engine=/src",
                "/e/runner-work-win/engine/engine=/src",
            ):
                self.assertIn(f"--remap-path-prefix {spelling}", flags)
            self.assertNotIn("\r", flags)

    def test_plain_directories_add_no_second_spelling(self):
        with tempfile.TemporaryDirectory(prefix="rrf_plain_") as tmp:
            root = pathlib.Path(tmp).resolve()
            (root / ".cargo" / "registry").mkdir(parents=True)
            rc, flags, err = _sourced_rustflags(
                home=str(root), workspace=str(root), sysroot=_UNSET,
            )
            self.assertEqual(rc, 0, err)
            self.assertEqual(flags.count("=/registry"), 1, flags)


class CCompilerMapTests(unittest.TestCase):
    """C/C++ sources get -ffile-prefix-map for the same pairs (#remap).

    cc-rs compiles never see --remap-path-prefix, so quickjs.c's __FILE__
    would ship the runner's registry path. cc-rs reads the most specific
    CFLAGS variant set, so a preset CFLAGS_<triple> must carry the map too.
    """

    HOME = _HOME + "ghr1"
    WS = "/srv/r1/_work/engine/engine"
    CARGO = _HOME + "ghr1/.cargo"

    def _var(self, var, **kw):
        kw.setdefault("home", self.HOME)
        kw.setdefault("workspace", self.WS)
        kw.setdefault("cargo_home", self.CARGO)
        kw.setdefault("sysroot", _UNSET)
        rc, out, err = _sourced_rustflags(print_var=var, **kw)
        self.assertEqual(rc, 0, err)
        return out

    def test_cflags_and_cxxflags_carry_every_pair_in_order(self):
        for var in ("CFLAGS", "CXXFLAGS"):
            flags = self._var(var)
            self.assertIn(f"-ffile-prefix-map={self.WS}=/src", flags)
            self.assertIn(f"-ffile-prefix-map={self.CARGO}/registry=/registry", flags)
            _assert_ordered(self, flags, ("=/buildenv", "=/src", "=/git", "=/registry"))
            self.assertNotIn("--remap-path-prefix", flags)

    def test_a_preset_target_variant_keeps_its_flags_and_gains_the_map(self):
        flags = self._var(
            "CFLAGS_aarch64_unknown_linux_gnu",
            extra_env={"CFLAGS_aarch64_unknown_linux_gnu": "-std=gnu99", "TARGET_CXXFLAGS": "-O2"},
        )
        self.assertTrue(flags.startswith("-std=gnu99 "), flags)
        self.assertIn(f"-ffile-prefix-map={self.WS}=/src", flags)
        self.assertIn(
            f"-ffile-prefix-map={self.WS}=/src",
            self._var("TARGET_CXXFLAGS", extra_env={"TARGET_CXXFLAGS": "-O2"}),
        )

    OLD_GCC = (
        "#!/bin/sh\n"
        'for a in "$@"; do case "$a" in -ffile-prefix-map=*|-fmacro-prefix-map=*) '
        'echo "gcc: error: unrecognized command line option $a" >&2; exit 1;; esac; done\n'
        "cat >/dev/null; exit 0\n"
    )
    NEW_GCC = "#!/bin/sh\ncat >/dev/null; exit 0\n"

    def test_a_compiler_without_file_prefix_map_gets_the_debug_map(self):
        # 15-10: gcc 4.8.5 in the manylinux2014 aarch64 cross image rejected
        # -ffile-prefix-map and failed the wheel build.
        rc, out, err = _sourced_rustflags(
            home=self.HOME, workspace=self.WS, cargo_home=self.CARGO, sysroot=_UNSET,
            extra_bins={"oldgcc": self.OLD_GCC, "cc": self.NEW_GCC},
            extra_env={"TARGET_CC": "oldgcc", "CFLAGS_aarch64_unknown_linux_gnu": "-std=gnu99"},
            print_var="CFLAGS_aarch64_unknown_linux_gnu",
        )
        self.assertEqual(rc, 0, err)
        self.assertIn(f"-fdebug-prefix-map={self.WS}=/src", out)
        self.assertNotIn("-ffile-prefix-map", out)
        _assert_ordered(self, out, ("=/buildenv", "=/src", "=/git", "=/registry"))
        self.assertIn("oldgcc rejects -ffile-prefix-map", err)

    def test_compilers_that_take_the_flag_keep_the_file_map(self):
        rc, out, err = _sourced_rustflags(
            home=self.HOME, workspace=self.WS, cargo_home=self.CARGO, sysroot=_UNSET,
            extra_bins={"newgcc": self.NEW_GCC, "cc": self.NEW_GCC},
            extra_env={"TARGET_CC": "newgcc"}, print_var="CFLAGS",
        )
        self.assertEqual(rc, 0, err)
        self.assertIn(f"-ffile-prefix-map={self.WS}=/src", out)
        self.assertNotIn("rejects", err)

    def test_sourcing_twice_adds_the_map_once(self):
        _, out, _ = _sourced_rustflags(
            home=self.HOME, workspace=self.WS, cargo_home=self.CARGO, sysroot=_UNSET,
            args=("", "&&", "source", str(HELPER)), print_var="CFLAGS",
        )
        self.assertEqual(out.count(f"-ffile-prefix-map={self.WS}=/src"), 1, out)

    def test_msvc_gets_no_map_but_windows_gnu_does(self):
        cyg = {"cygpath": "#!/bin/sh\nprintf '%s\\n' \"$2\"\n"}
        self.assertEqual(self._var("CFLAGS", extra_bins=cyg), "")
        gnu = self._var("CFLAGS", extra_bins=cyg, args=("x86_64-pc-windows-gnu",))
        self.assertIn(f"-ffile-prefix-map={self.WS}=/src", gnu)


class PerTargetExtrasTests(unittest.TestCase):
    """Env flags replace the config outright, so the helper repeats the extras."""

    FAKE = LinuxRunnerTests

    def _flags(self, **kwargs):
        params = dict(
            home=self.FAKE.LINUX_HOME,
            workspace=self.FAKE.LINUX_WS,
            cargo_home=self.FAKE.LINUX_CARGO,
            sysroot=self.FAKE.LINUX_SYSROOT,
        )
        params.update(kwargs)
        rc, flags, err = _sourced_rustflags(**params)
        self.assertEqual(rc, 0, err)
        return flags

    def test_bare_sourcing_carries_no_extra_flag(self):
        self.assertNotIn("-C target-feature", self._flags())

    def test_wasm32_keeps_the_simd_flags(self):
        flags = self._flags(args=("wasm32-unknown-unknown",))
        self.assertIn("-C target-feature=+simd128,+bulk-memory", flags)
        self.assertNotIn("crt-static", flags)

    def test_musl_triple_still_takes_no_crt_static(self):
        # The P0 split at runtime: a musl target through the general helper
        # takes only remaps. The CRT flag rides the musl wrapper alone, so
        # the musl CLI — which sources this too — stays statically linked.
        for triple in ("x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"):
            flags = self._flags(args=(triple,))
            self.assertNotIn("crt-static", flags)
            self.assertNotIn("simd128", flags)

    def test_musl_wrapper_adds_only_the_crt_flag(self):
        rc, flags, err = _sourced_rustflags(
            home=self.FAKE.LINUX_HOME,
            workspace=self.FAKE.LINUX_WS,
            cargo_home=self.FAKE.LINUX_CARGO,
            sysroot=self.FAKE.LINUX_SYSROOT,
            helper=MUSL_HELPER,
        )
        self.assertEqual(rc, 0, err)
        self.assertIn("-C target-feature=-crt-static", flags)
        for rule in (
            f"{self.FAKE.LINUX_HOME}=/buildenv",
            f"{self.FAKE.LINUX_WS}=/src",
            f"{self.FAKE.LINUX_CARGO}/git=/git",
            f"{self.FAKE.LINUX_CARGO}/registry=/registry",
            f"{self.FAKE.LINUX_SYSROOT}=/rustc",
        ):
            self.assertIn(rule, flags)

    def test_pre_existing_rustflags_are_kept_in_front(self):
        flags = self._flags(rustflags="-C opt-level=3")
        self.assertTrue(flags.startswith("-C opt-level=3 --remap-path-prefix"), flags)

    def test_sourcing_twice_keeps_every_rule(self):
        # The release steps source once at the top and the musl subshells
        # source the wrapper on top of that: repeats must stay harmless.
        with tempfile.TemporaryDirectory(prefix="rrf_twice_") as tmp:
            bindir = pathlib.Path(tmp) / "bin"
            bindir.mkdir()
            rustc = bindir / "rustc"
            rustc.write_text(f"#!/bin/sh\nprintf '%s\\n' '{self.FAKE.LINUX_SYSROOT}'\n")
            rustc.chmod(0o755)
            env = dict(os.environ)
            for var in ("CARGO_HOME", "USERPROFILE", "RUSTUP_HOME", "RUSTFLAGS"):
                env.pop(var, None)
            env["HOME"] = self.FAKE.LINUX_HOME
            env["GITHUB_WORKSPACE"] = self.FAKE.LINUX_WS
            env["CARGO_HOME"] = self.FAKE.LINUX_CARGO
            env["PATH"] = f"{bindir}{os.pathsep}{env.get('PATH', '')}"
            proc = subprocess.run(
                [
                    "bash",
                    "-c",
                    f"source {HELPER} && source {MUSL_HELPER} "
                    '&& printf "%s" "$RUSTFLAGS"',
                ],
                capture_output=True,
                text=True,
                timeout=30,
                env=env,
                cwd=str(REPO),
            )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        for rule in (
            f"{self.FAKE.LINUX_HOME}=/buildenv",
            f"{self.FAKE.LINUX_WS}=/src",
            f"{self.FAKE.LINUX_CARGO}/git=/git",
            f"{self.FAKE.LINUX_CARGO}/registry=/registry",
            f"{self.FAKE.LINUX_SYSROOT}=/rustc",
        ):
            self.assertIn(rule, proc.stdout)
        self.assertIn("-C target-feature=-crt-static", proc.stdout)


class RepoRootTests(unittest.TestCase):
    """Without a workspace the root still resolves; gaps degrade, never break."""

    FAKE = LinuxRunnerTests

    def test_git_toplevel_feeds_src_from_any_subdirectory(self):
        if shutil.which("git") is None:
            self.skipTest("git is not installed here")
        toplevel = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=str(REPO),
            env=_git_env(),
        ).stdout.strip()
        self.assertTrue(toplevel)
        for cwd in (REPO, REPO / "crates" / "pdf-node"):
            rc, flags, err = _sourced_rustflags(
                home=self.FAKE.LINUX_HOME,
                cargo_home=self.FAKE.LINUX_CARGO,
                sysroot=self.FAKE.LINUX_SYSROOT,
                cwd=cwd,
            )
            self.assertEqual(rc, 0, err)
            # Physical paths: git spells the root physically while the
            # fallback route may arrive through a symlink — same directory.
            self.assertEqual(
                os.path.realpath(_src_of(self, flags)),
                os.path.realpath(toplevel),
            )
        # The push-gate red: the pre-push hook exports GIT_DIR naming the
        # real git dir, and with GIT_DIR set and no GIT_WORK_TREE rev-parse
        # answers the current directory — from a subdirectory the helper
        # mapped that subdirectory to /src. It must still emit the true root.
        git_dir = subprocess.run(
            ["git", "rev-parse", "--absolute-git-dir"],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=str(REPO),
            env=_git_env(),
        ).stdout.strip()
        self.assertTrue(git_dir)
        rc, flags, err = _sourced_rustflags(
            home=self.FAKE.LINUX_HOME,
            cargo_home=self.FAKE.LINUX_CARGO,
            sysroot=self.FAKE.LINUX_SYSROOT,
            git_dir=git_dir,
            cwd=REPO / "crates" / "pdf-node",
        )
        self.assertEqual(rc, 0, err)
        self.assertEqual(
            os.path.realpath(_src_of(self, flags)),
            os.path.realpath(toplevel),
        )

    def test_dubious_ownership_falls_back_to_the_physical_root(self):
        # The AC push-gate red: the checkout is owned by another user and
        # the scrubbed builder env carries no safe.directory config, so git
        # refuses with exit 128 — and the helper's own path may spell the
        # root through a symlink (/tmp -> /private/tmp). The fallback must
        # still emit the physical root, exactly as git would spell it.
        with tempfile.TemporaryDirectory(prefix="rrf_dubious_") as tmp:
            link = pathlib.Path(tmp) / "linked-checkout"
            link.symlink_to(REPO, target_is_directory=True)
            helper = link / "scripts" / "release" / "release_rustflags.sh"
            self.assertTrue(helper.is_file())
            rc, flags, err = _sourced_rustflags(
                home=self.FAKE.LINUX_HOME,
                cargo_home=self.FAKE.LINUX_CARGO,
                sysroot=self.FAKE.LINUX_SYSROOT,
                git_dubious=True,
                helper=helper,
                cwd=link,
            )
        self.assertEqual(rc, 0, err)
        self.assertIn(f"{os.path.realpath(REPO)}=/src", flags)

    def test_without_git_the_helper_s_own_tree_feeds_src(self):
        with tempfile.TemporaryDirectory(prefix="rrf_nogit_") as tmp:
            rc, flags, err = _sourced_rustflags(
                home=self.FAKE.LINUX_HOME,
                cargo_home=self.FAKE.LINUX_CARGO,
                sysroot=self.FAKE.LINUX_SYSROOT,
                git_ok=False,
                cwd=tmp,
            )
        self.assertEqual(rc, 0, err)
        self.assertEqual(
            os.path.realpath(_src_of(self, flags)),
            os.path.realpath(REPO),
        )

    def test_missing_pieces_are_skipped_not_fatal(self):
        rc, flags, err = _sourced_rustflags(
            workspace=self.FAKE.LINUX_WS,
            cargo_home=self.FAKE.LINUX_CARGO,
        )
        self.assertEqual(rc, 0, err)
        self.assertNotIn("=/buildenv", flags)
        self.assertNotIn("=/rustc", flags)
        self.assertIn(f"{self.FAKE.LINUX_WS}=/src", flags)
        self.assertIn(f"{self.FAKE.LINUX_CARGO}/git=/git", flags)


# A step that compiles a shippable Rust artefact: installing a tool (cargo
# install), reading metadata, or driving a smoke through an npm script is not
# one, and must not be flagged.
BUILD_COMMANDS = (
    "cargo build",
    "cargo zigbuild",
    "cross build",
    "maturin build",
    "napi build",
    "wasm-pack build",
)

FOUR = (
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
    "wasm.yml",
)

# Every artefact-producing step, by (workflow, job, step). A new build step
# fails the scan until it is registered here WITH its sourcing — an artefact
# that compiles unsourced is the leak this file exists to stop.
EXPECTED_BUILD_STEPS = {
    "release-natives.yml": [
        ("natives", "Build the cdylibs"),
        ("natives", "Build the wheel"),
    ],
    "release-binaries.yml": [
        ("build", "Build binary"),
        ("create-release", "Build host binary for the release gate"),
    ],
    "node-bindings.yml": [
        ("build-release", "Build for ${{ matrix.target }}"),
    ],
    "wasm.yml": [
        ("wasm-build", "Build wasm package"),
    ],
}

# The P0 split at workflow level: musl cdylib steps keep the musl wrapper
# (they cannot link without the CRT flag); the musl CLI and wasm steps must
# never take it (the CLI must stay statically linked).
MUST_TAKE_MUSL_WRAPPER = {
    ("release-natives.yml", "natives", "Build the cdylibs"),
    ("release-natives.yml", "natives", "Build the wheel"),
    ("node-bindings.yml", "build-release", "Build for ${{ matrix.target }}"),
}
MUST_NOT_TAKE_MUSL_WRAPPER = {
    ("release-binaries.yml", "build", "Build binary"),
    ("release-binaries.yml", "create-release", "Build host binary for the release gate"),
    ("wasm.yml", "wasm-build", "Build wasm package"),
}

MINIMUM_CASES = 6  # FLOOR


def _load(name):
    return yaml.safe_load((WORKFLOWS / name).read_text())


def _steps(doc):
    for job_name, job in doc["jobs"].items():
        for step in job.get("steps") or []:
            if isinstance(step, dict) and isinstance(step.get("run"), str):
                yield job_name, step


def _code_lines(run):
    """The run block without full-line comments: mentions there source nothing."""
    return "\n".join(
        line for line in run.splitlines() if not line.strip().startswith("#")
    )


SOURCE_GENERAL = re.compile(r"source\s+\S*release_rustflags\.sh(\s|;|&|$)")
SOURCE_MUSL = re.compile(r"source\s+\S*musl_cdylib_rustflags\.sh(\s|;|&|$)")


def _scanned_build_steps():
    """Every (workflow, job, step) whose run block compiles an artefact."""
    found = []
    for name in FOUR:
        for job_name, step in _steps(_load(name)):
            if any(cmd in _code_lines(step["run"]) for cmd in BUILD_COMMANDS):
                found.append((name, job_name, step.get("name", "")))
    return found


def _run_of(name, job_name, step_name):
    for found_job, step in _steps(_load(name)):
        if found_job == job_name and step.get("name") == step_name:
            return step
    raise AssertionError(f"{name} {job_name} {step_name!r} not found")


class WorkflowShapeTests(unittest.TestCase):
    def test_every_build_step_is_registered(self):
        found = _scanned_build_steps()
        self.assertGreaterEqual(len(found), MINIMUM_CASES)
        expected = [
            (name, job, step)
            for name in FOUR
            for job, step in EXPECTED_BUILD_STEPS[name]
        ]
        self.assertEqual(
            sorted(found),
            sorted(expected),
            "a build step appeared, moved or went away: register it WITH its "
            "sourcing, or say why it compiles no artefact",
        )

    def test_every_build_step_sources_the_helper_before_building(self):
        for name in FOUR:
            for job_name, step_name in EXPECTED_BUILD_STEPS[name]:
                code = _code_lines(_run_of(name, job_name, step_name)["run"])
                with self.subTest(f"{name} {job_name} {step_name}"):
                    match = SOURCE_GENERAL.search(code)
                    self.assertIsNotNone(
                        match, "no `source .../release_rustflags.sh` before the build"
                    )
                    first_build = min(
                        code.index(cmd) for cmd in BUILD_COMMANDS if cmd in code
                    )
                    self.assertLess(
                        match.start(),
                        first_build,
                        "the helper is sourced after the build starts",
                    )

    def test_every_build_step_pins_bash(self):
        # `source` is a bashism and three of these matrices include Windows,
        # whose default shell is PowerShell.
        for name in FOUR:
            for job_name, step_name in EXPECTED_BUILD_STEPS[name]:
                step = _run_of(name, job_name, step_name)
                with self.subTest(f"{name} {job_name} {step_name}"):
                    self.assertEqual(step.get("shell"), "bash")

    def test_musl_cdylib_steps_keep_the_wrapper_and_the_cli_never_takes_it(self):
        for name, job_name, step_name in MUST_TAKE_MUSL_WRAPPER:
            with self.subTest(f"{name} {step_name}"):
                self.assertIsNotNone(
                    SOURCE_MUSL.search(
                        _code_lines(_run_of(name, job_name, step_name)["run"])
                    ),
                    "a musl cdylib without the wrapper cannot link",
                )
        for name, job_name, step_name in MUST_NOT_TAKE_MUSL_WRAPPER:
            with self.subTest(f"{name} {step_name}"):
                self.assertNotIn(
                    "musl_cdylib_rustflags.sh",
                    _run_of(name, job_name, step_name)["run"],
                    "the musl CLI must stay statically linked",
                )

    def test_wasm_passes_its_triple_for_the_simd_flags(self):
        code = _code_lines(_run_of("wasm.yml", "wasm-build", "Build wasm package")["run"])
        self.assertIn("release_rustflags.sh wasm32-unknown-unknown", code)


if __name__ == "__main__":
    unittest.main()
