#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The musl CRT flag rides only the cdylibs; the CLI stays static (P0).

`.cargo/config.toml` used to carry `-C target-feature=-crt-static` on the
whole musl target. That file cannot tell a cdylib from a binary, so the musl
CLI came out dynamically linked: the Docker image (Debian, no ld-musl) and
every "runs anywhere" user broke. The flag now lives in
`scripts/release/musl_cdylib_rustflags.sh`, sourced per musl cdylib build, and
these cases hold the split: the config must not name the flag, the helper
must export it beside the path scrubbing, the cdylib lane must source the
helper, and the CLI lane must not.
"""
from __future__ import annotations

import os
import pathlib
import re
import subprocess
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
CONFIG = REPO / ".cargo" / "config.toml"
HELPER = REPO / "scripts" / "release" / "musl_cdylib_rustflags.sh"
RELEASE_HELPER = REPO / "scripts" / "release" / "release_rustflags.sh"
LANE = REPO / "scripts" / "release" / "build_all_platforms.sh"
CLI_BUILD = REPO / "scripts" / "release" / "cli_build_archives.sh"

MINIMUM_CASES = 6  # FLOOR

MUSL_SECTIONS = (
    "target.x86_64-unknown-linux-musl",
    "target.aarch64-unknown-linux-musl",
)

# The four remap targets the musl sections must keep scrubbing. The config
# pins one machine's home (allowed: .cargo/config.toml is on the snapshot
# audit's list with its reason); the test matches the mapping, not the home,
# so no machine path is quoted here.
SCRUB_TARGETS = ("=/buildenv", "=/src", "=/git", "=/registry")

# The helper's form of the same block: derived from the builder's environment
# at source time, so it scrubs on whatever machine builds rather than only the
# one whose path is in the config. The catch-all and the default cargo home
# come from $HOME; /src maps the repo root (the workspace here), not one
# checkout; the sysroot maps the toolchain rustc reports.
HELPER_ENV_FORMS = (
    "$HOME=/buildenv",
    "$GITHUB_WORKSPACE=/src",
    "$HOME/.cargo/git=/git",
    "$HOME/.cargo/registry=/registry",
)


def _section(text: str, name: str) -> str:
    """The body of one `[target.<name>]` TOML section."""
    match = re.search(
        rf"^\[target\.{re.escape(name.split('.', 1)[1])}\]\n(.*?)(?=^\[|\Z)",
        text,
        re.M | re.S,
    )
    assert match is not None, f"[{name}] missing from .cargo/config.toml"
    return match.group(1)


class MuslCrtSplitTests(unittest.TestCase):
    def test_musl_target_sections_carry_no_crt_static_flag(self):
        text = CONFIG.read_text()
        for section in MUSL_SECTIONS:
            body = _section(text, section)
            self.assertNotIn(
                "crt-static",
                body,
                f"[{section}] must not set -crt-static: the flag would "
                "dynamically link the musl CLI too",
            )

    def test_musl_target_sections_keep_linker_and_scrubbing(self):
        text = CONFIG.read_text()
        x86 = _section(text, MUSL_SECTIONS[0])
        self.assertIn('linker = "musl-gcc"', x86)
        for section in MUSL_SECTIONS:
            body = _section(text, section)
            for target in SCRUB_TARGETS:
                self.assertRegex(
                    body,
                    r'"[^"\s]+' + re.escape(target) + r'"',
                    f"[{section}] lost a path-scrubbing prefix: target flags "
                    "replace [build] flags rather than merging",
                )

    def test_helper_exports_crt_static_beside_the_scrubbing(self):
        body = HELPER.read_text()
        self.assertIn("target-feature=-crt-static", body)
        # The remap block lives one file over: the musl helper sources the
        # general release helper, so the block is computed once, from the
        # builder's own paths, for every release build — not only musl cdylibs.
        self.assertIn("release_rustflags.sh", body)
        release = RELEASE_HELPER.read_text()
        for target in ('"/buildenv"', '"/src"', '"/git"', '"/registry"', '"/rustc"'):
            self.assertIn(
                target,
                release,
                "the release helper must carry the remap block: env RUSTFLAGS "
                "replace the config's flags outright",
            )
        for source in ("GITHUB_WORKSPACE", "CARGO_HOME", "rustc --print sysroot"):
            self.assertIn(
                source,
                release,
                "the block is derived from the builder's environment, not one "
                "machine's paths",
            )
        # The split at its source: the general helper must never emit the
        # flag, whatever target it is sourced for — the musl CLI sources it
        # too and must stay statically linked.
        self.assertNotIn("crt-static", release)
        # And neither file names a machine's home: a literal there would ship
        # a private path in the published tree and scrub nothing anywhere
        # else. The audit's home-dir alternatives, not its full pattern:
        # the full one names paths this file must not quote either.
        self.assertNotRegex(body, r"/(Users|home)/[a-z]")
        self.assertNotRegex(release, r"/(Users|home)/[a-z]")
        env = dict(os.environ)
        for var in ("CARGO_HOME", "USERPROFILE", "RUSTUP_HOME", "RUSTFLAGS"):
            env.pop(var, None)
        proc = subprocess.run(
            [
                "bash",
                "-c",
                "HOME=/from/the/env GITHUB_WORKSPACE=/from/the/env/ws "
                f"source {HELPER} "
                "&& printf '%s' \"$RUSTFLAGS\"",
            ],
            capture_output=True,
            text=True,
            timeout=30,
            env=env,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("target-feature=-crt-static", proc.stdout)
        for form in HELPER_ENV_FORMS:
            self.assertIn(
                form.replace("$HOME", "/from/the/env").replace(
                    "$GITHUB_WORKSPACE", "/from/the/env/ws"
                ),
                proc.stdout,
            )

    def test_cdylib_lane_sources_the_helper_for_musl(self):
        body = LANE.read_text()
        hits = body.count("source scripts/release/musl_cdylib_rustflags.sh")
        # Python musl wheels, C API musl libs, napi musl .node: three musl
        # cdylib builds, each in a subshell so the flag leaks nowhere.
        self.assertGreaterEqual(hits, 3, body)
        for triple_gate in ("*-musl",):
            self.assertIn(triple_gate, body)

    def test_cli_build_never_takes_the_cdylib_flags(self):
        body = CLI_BUILD.read_text()
        self.assertNotIn("musl_cdylib_rustflags", body)
        self.assertNotIn("crt-static", body)
        # And it still refuses a dynamic musl binary: reading the binary's
        # own headers through the gate, not the host's loader answers.
        self.assertIn("elf_is_static", body)
        self.assertIn("dynamically linked", body)
        self.assertNotIn("command -v ldd", body)

    def test_no_cargo_config_sets_crt_static_anywhere(self):
        # The split has exactly one home for the flag. A second setting —
        # here or in any other config cargo reads — would reintroduce the
        # dynamic CLI on whatever target it names.
        for config in [CONFIG, *sorted(REPO.glob("**/.cargo/config.toml"))]:
            if not config.is_file():
                continue
            for i, line in enumerate(config.read_text().splitlines(), 1):
                stripped = line.split("#", 1)[0]
                self.assertNotIn(
                    "crt-static",
                    stripped,
                    f"{config.relative_to(REPO)}:{i} sets -crt-static",
                )


if __name__ == "__main__":
    unittest.main()
