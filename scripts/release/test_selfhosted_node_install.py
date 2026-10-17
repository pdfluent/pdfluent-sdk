#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The node tarball install lands once and keeps what it landed.

selfhosted_setup.sh installs node from the nodejs.org tarball, which already
extracts to node-<ver>-<dist>/. Extracting straight into $HOME/tools and
moving that dir onto its own name moves it into itself, and mv refuses (run
37306831270). The install stages the tarball in a temp dir and moves the
single top-level dir into place instead -- and when the target already holds
that version it downloads nothing and keeps it.

The install runs here for real, twice, against a stubbed curl serving a local
index and a local tarball with the real layout, in a temp HOME with node kept
off PATH. The first run lands the toolchain; the second succeeds without
fetching the tarball again and leaves the target byte-identical. Both runs
fail against the old self-moving mv.
"""
from __future__ import annotations

import hashlib
import io
import json
import os
import platform
import pathlib
import shutil
import stat
import subprocess
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

VERSION = "v20.99.0"
MAJOR = "20"
NPM_VERSION = "10.99.0"

CURL_STUB = """#!/bin/sh
# fetch() calls curl with -o DEST and the URL last; serve the fixtures.
dest="" url="" prev=""
for a in "$@"; do
  if [ "$prev" = "-o" ]; then dest="$a"; prev=""; continue; fi
  case "$a" in -o) prev="-o";; -*) ;; *) url="$a";; esac
done
echo "$url" >> "$CURL_CALLS_LOG"
case "$url" in
  *index.json) cp "$NODE_INDEX_FIXTURE" "$dest" ;;
  *) cp "$NODE_TARBALL_FIXTURE" "$dest" ;;
esac
"""

# Kept off the scrubbed PATH so the install always runs: a node of the
# wanted major would take the already-present early return instead.
HIDDEN_TOOLS = {"node", "npm", "curl"}


def node_dist() -> str:
    """The setup's node_arch() mapping, for this machine."""
    out = subprocess.run(
        ["uname", "-s", "-m"], capture_output=True, text=True, check=True
    ).stdout.split()
    mapping = {
        ("Linux", "x86_64"): "linux-x64",
        ("Linux", "aarch64"): "linux-arm64",
        ("Linux", "arm64"): "linux-arm64",
        ("Darwin", "arm64"): "darwin-arm64",
        ("Darwin", "aarch64"): "darwin-arm64",
        ("Darwin", "x86_64"): "darwin-x64",
    }
    return mapping[(out[0], out[1])]


def snapshot_tree(root: pathlib.Path) -> dict[str, str]:
    """Every file under root, as relpath -> sha256."""
    found: dict[str, str] = {}
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            found[str(path.relative_to(root))] = hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
    return found


# On Darwin the dispatch takes setup_darwin_node (pinned tarballs under
# $PDFLUENT_TOOLS, test_selfhosted_setup_darwin.py), never this generic
# setup_node; this case runs where that path does, on Linux.
@unittest.skipIf(platform.system() == "Darwin", "Darwin runs setup_darwin_node, not setup_node")
class NodeInstallTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        tmp = pathlib.Path(self.tmp.name)
        self.home = tmp / "home"
        self.home.mkdir()
        self.dist = node_dist()
        self.top = f"node-{VERSION}-{self.dist}"
        self.dest = self.home / "tools" / self.top

        tarball = tmp / "node.tar.xz"
        with tarfile.open(tarball, "w:xz") as tar:
            for name, body in (
                ("bin/node", f"#!/bin/sh\necho '{VERSION}'\n"),
                ("bin/npm", f"#!/bin/sh\necho '{NPM_VERSION}'\n"),
            ):
                data = body.encode()
                info = tarfile.TarInfo(f"{self.top}/{name}")
                info.size = len(data)
                info.mode = 0o755
                info.mtime = 1700000000
                tar.addfile(info, fileobj=io.BytesIO(data))
        self.tarball = tarball
        self.index = tmp / "index.json"
        self.index.write_text(json.dumps([{"version": VERSION}]))
        self.calls = tmp / "curl-calls.log"

        # A PATH without node, npm or curl, so the install cannot take the
        # already-present early return and cannot reach the network; every
        # other tool links through to the real one.
        self.bindir = tmp / "bin"
        self.bindir.mkdir()
        for entry in os.environ.get("PATH", "").split(os.pathsep):
            if not entry or not os.path.isdir(entry):
                continue
            for name in sorted(os.listdir(entry)):
                if name in HIDDEN_TOOLS or (self.bindir / name).exists():
                    continue
                source = os.path.join(entry, name)
                if os.path.isdir(source) and not os.path.islink(source):
                    continue
                try:
                    os.symlink(source, self.bindir / name)
                except FileExistsError:
                    pass
        curl = self.bindir / "curl"
        curl.write_text(CURL_STUB)
        curl.chmod(curl.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        self.bash = shutil.which("bash")
        self.assertIsNotNone(self.bash, "bash is missing")

    def run_setup(self) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env["HOME"] = str(self.home)
        env["PATH"] = str(self.bindir)
        env["CURL_CALLS_LOG"] = str(self.calls)
        env["NODE_INDEX_FIXTURE"] = str(self.index)
        env["NODE_TARBALL_FIXTURE"] = str(self.tarball)
        # The setup appends to these when set; the test must not write the
        # real workflow files when it runs inside one.
        env.pop("GITHUB_PATH", None)
        env.pop("GITHUB_ENV", None)
        return subprocess.run(
            [self.bash, str(SCRIPT), "--tools", "node", "--node-version", MAJOR],
            capture_output=True,
            text=True,
            cwd=REPO,
            env=env,
        )

    def test_install_then_keep(self):
        first = self.run_setup()
        self.assertEqual(first.returncode, 0, first.stderr[-2000:])
        self.assertIn(f"installing node {VERSION} ({self.dist})", first.stderr)
        self.assertTrue((self.dest / "bin" / "node").is_file())
        node = subprocess.run(
            [str(self.dest / "bin" / "node"), "--version"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(node.stdout.strip(), VERSION)
        before = snapshot_tree(self.dest)
        self.assertTrue(before, "the install must land files")
        fetched = self.calls.read_text().splitlines()
        self.assertEqual(
            sum("index.json" not in call for call in fetched),
            1,
            f"the first run fetches the tarball once, fetched: {fetched}",
        )

        second = self.run_setup()
        self.assertEqual(second.returncode, 0, second.stderr[-2000:])
        # On Linux the setup's tool-dir pickup puts the landed node on PATH
        # before the checks, so the second run takes the already-present
        # early return; elsewhere it reaches the target guard and keeps it.
        # Either way it installs nothing.
        self.assertTrue(
            "keeping it" in second.stderr
            or "(already present)" in second.stderr,
            second.stderr[-2000:],
        )
        fetched = self.calls.read_text().splitlines()
        self.assertEqual(
            sum("index.json" not in call for call in fetched),
            1,
            f"the second run fetches no tarball, fetched: {fetched}",
        )
        self.assertEqual(
            snapshot_tree(self.dest), before, "the second run touches nothing"
        )

    def test_no_self_move(self):
        text = SCRIPT.read_text()
        start = text.index("setup_node() {")
        end = text.index("\n}\n", start)
        body = text[start:end]
        self.assertNotIn(
            'mv "$HOME/tools/node-${ver}-${dist}" "$dest"',
            body,
            "the tarball already extracts to that name: the mv moves it into itself",
        )
        self.assertIn('[ -e "$dest" ]', body)
        self.assertIn("keeping it", body)


if __name__ == "__main__":
    unittest.main()
