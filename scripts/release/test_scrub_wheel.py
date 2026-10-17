#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""scrub-wheel.py leaves no absolute build-host path in a wheel's SBOM.

The 1.1.0 CI wheels carried 203 such paths each, in four shapes (linux
`/srv/r1/_work/...`, macOS `/Volumes/.../actions-runner-mac-work/...`,
Windows `C:/...` and `C:\\...`), where the 1.0.0 wheels carried none. The
old scrub matched only `/Users|home|private|root|workspace|builds?/`: it
left the linux and Windows paths alone, cut the macOS ones in half
(`/Volumes/BuildDisk/ci./crates/...`) and still counted 0.

The fixtures are synthetic wheels built here, one SBOM string per shape
measured on the real wheels, plus developer and cargo-source shapes. The
script runs as a subprocess, the way audit-all-packages.sh calls it; set
SCRUB_WHEEL_SCRIPT to point the tests at another copy of the script.
"""
from __future__ import annotations

import base64
import csv
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = pathlib.Path(os.environ.get("SCRUB_WHEEL_SCRIPT", HERE / "scrub-wheel.py"))

DIST = "pdfluent-1.1.0.dist-info"
SBOM = f"{DIST}/sboms/pdf-python.cyclonedx.json"

# Independent of the script: any absolute path a consumer would see.
HOST = re.compile(
    r"file:///?(?!\.(?:/|$|[\s\"#?]))[^\s\"#?]+"
    r"|(?<![A-Za-z0-9])[A-Za-z]:[\\/]"
    r"|/(?:srv|Volumes|Users|home|builds|actions-runner)[/\\]"
    r"|actions-runner-|_work[/\\]"
)

# (input, expected) — every input shape was measured on a 1.1.0 CI wheel
# except where noted.
CASES = {
    "linux": (
        "path+file:///srv/r1/_work/engine/engine/crates/pdf-python#1.1.0",
        "path+file://./crates/pdf-python#1.1.0",
    ),
    "macos": (
        "path+file:///Volumes/BuildDisk/ci/builds/runner-work/engine/engine/crates/cff-parser#pdfluent-cff@0.2.2",
        "path+file://./crates/cff-parser#pdfluent-cff@0.2.2",
    ),
    "windows_fwd": (
        "path+file:///C:/actions-runner-ci/_work/engine/engine/crates/pdf-engine#1.1.0",
        "path+file://./crates/pdf-engine#1.1.0",
    ),
    "windows_back_purl": (
        "pkg:cargo/pdfluent-cff@0.2.2?download_url=file://C:\\actions-runner-ci\\_work\\engine\\engine\\crates\\cff-parser",
        "pkg:cargo/pdfluent-cff@0.2.2?download_url=file://./crates/cff-parser",
    ),
    "linux_purl": (
        "pkg:cargo/pdf-annot@1.1.0?download_url=file:///srv/r1/_work/engine/engine/crates/pdf-annot",
        "pkg:cargo/pdf-annot@1.1.0?download_url=file://./crates/pdf-annot",
    ),
    # developer machines (not on the CI wheels)
    "dev_users": (
        "path+file:///Use" "rs/someone/src/engine/crates/pdf-render#1.1.0",
        "path+file://./crates/pdf-render#1.1.0",
    ),
    "dev_home": (
        "path+file:///ho" "me/someone/engine/crates/pdf-render#1.1.0",
        "path+file://./crates/pdf-render#1.1.0",
    ),
    # cargo sources (not on the CI wheels)
    "registry": (
        "pkg:cargo/serde@1.0.200?download_url=file:///ho" "me/someone/.cargo/registry/src/index.crates.io-6f17d22bba15001f/serde-1.0.200",
        "pkg:cargo/serde@1.0.200?download_url=file://./cargo-registry/serde-1.0.200",
    ),
    "git": (
        "pkg:cargo/hayro@0.7.0?download_url=file:///Use" "rs/someone/.cargo/git/checkouts/hayro-1a2b3c/4d5e6f7",
        "pkg:cargo/hayro@0.7.0?download_url=file://./cargo-git/hayro-1a2b3c/4d5e6f7",
    ),
}

# Never touched: not a host path.
KEEP = [
    "https://github.com/BurntSushi/jiff/tree/master/crates/jiff",
    "path+file://./crates/pdf-python#1.1.0",
    "pkg:cargo/serde@1.0.200",
    # cargo-cyclonedx's own relative form for the root crate (on the CI wheels)
    "pkg:cargo/pdf-python@1.1.0?download_url=file://.",
    "pkg:cargo/pdf-python@1.1.0?download_url=file://.#src/lib.rs",
]


def _record_line(name: str, data: bytes) -> str:
    h = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
    return f"{name},sha256={h},{len(data)}"


def make_wheel(path: pathlib.Path, strings: list[str]) -> None:
    sbom = {
        "bomFormat": "CycloneDX",
        "metadata": {"component": {"bom-ref": strings[0], "name": "pdfluent"}},
        "components": [{"bom-ref": s, "purl": s, "name": f"c{i}"} for i, s in enumerate(strings)],
        "dependencies": [{"ref": strings[0], "dependsOn": list(strings)}],
    }
    members = {
        "pdfluent/__init__.py": b"__version__ = '1.1.0'\n",
        "pdfluent/_native.abi3.so": bytes(range(256)) * 4,
        f"{DIST}/METADATA": b"Metadata-Version: 2.4\nName: pdfluent\nVersion: 1.1.0\n",
        f"{DIST}/WHEEL": b"Wheel-Version: 1.0\nRoot-Is-Purelib: false\nTag: cp38-abi3-any\n",
        SBOM: json.dumps(sbom).encode(),
    }
    record = "\n".join(_record_line(n, d) for n, d in members.items()) + f"\n{DIST}/RECORD,,\n"
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for n, d in members.items():
            z.writestr(n, d)
        z.writestr(f"{DIST}/RECORD", record)


def run(wheel: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(SCRIPT), str(wheel)], capture_output=True, text=True)


def sbom_strings(wheel: pathlib.Path) -> list[str]:
    out: list[str] = []

    def walk(o):
        if isinstance(o, dict):
            for k, v in o.items():
                out.append(k)
                walk(v)
        elif isinstance(o, list):
            for v in o:
                walk(v)
        elif isinstance(o, str):
            out.append(o)

    with zipfile.ZipFile(wheel) as z:
        walk(json.loads(z.read(SBOM)))
    return out


class ScrubWheel(unittest.TestCase):
    def setUp(self):
        self._td = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self._td.name)

    def tearDown(self):
        self._td.cleanup()

    def wheel(self, strings: list[str], name: str = "pdfluent-1.1.0-cp38-abi3-any.whl") -> pathlib.Path:
        p = self.dir / name
        make_wheel(p, strings)
        return p

    def test_each_host_path_shape_becomes_relative(self):
        for label, (given, want) in CASES.items():
            with self.subTest(shape=label):
                w = self.wheel([given])
                r = run(w)
                self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
                got = sbom_strings(w)
                self.assertIn(want, got)
                self.assertEqual([s for s in got if HOST.search(s)], [], f"{label}: host path left")

    def test_all_shapes_in_one_sbom_and_the_counter_counts_them(self):
        w = self.wheel([g for g, _ in CASES.values()] + KEEP)
        r = run(w)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual([s for s in sbom_strings(w) if HOST.search(s)], [])
        summary = json.loads(r.stdout)
        # each shape string appears 3x as a value (bom-ref/ref, purl, dependsOn)
        # except the first, which is also metadata.component.bom-ref and ref
        n = len(CASES)
        expected = 3 * n + 2
        self.assertEqual(summary["host_paths_rewritten"], expected)
        self.assertEqual(summary["host_paths_before"], expected)
        self.assertEqual(summary["host_paths_after"], 0)

    def test_urls_and_relative_paths_are_left_alone(self):
        w = self.wheel(KEEP)
        r = run(w)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        got = sbom_strings(w)
        for s in KEEP:
            self.assertIn(s, got)
        self.assertEqual(json.loads(r.stdout)["host_paths_rewritten"], 0)

    def test_second_run_changes_nothing(self):
        w = self.wheel([g for g, _ in CASES.values()])
        self.assertEqual(run(w).returncode, 0)
        first = w.read_bytes()
        r = run(w)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(w.read_bytes(), first)

    def test_record_matches_every_member_and_nothing_else_changed(self):
        w = self.wheel([g for g, _ in CASES.values()])
        with zipfile.ZipFile(w) as z:
            before = {n: z.read(n) for n in z.namelist()}
        self.assertEqual(run(w).returncode, 0)
        with zipfile.ZipFile(w) as z:
            self.assertIsNone(z.testzip())
            after = {n: z.read(n) for n in z.namelist()}
            rec = list(csv.reader(z.read(f"{DIST}/RECORD").decode().splitlines()))
        self.assertEqual(sorted(before), sorted(after))
        changed = sorted(n for n in before if before[n] != after[n])
        self.assertEqual(changed, sorted([SBOM, f"{DIST}/RECORD"]))
        listed = set()
        for row in rec:
            if not row:
                continue
            name, digest, size = row
            listed.add(name)
            if name == f"{DIST}/RECORD":
                continue
            self.assertEqual(f"{name},{digest},{size}", _record_line(name, after[name]), name)
        self.assertEqual(listed, set(after))

    def test_a_path_it_cannot_rewrite_fails_and_names_it(self):
        bad = "pkg:generic/libfoo@1?download_url=file:///opt/toolchain/lib/libfoo.so"
        w = self.wheel([CASES["linux"][0], bad])
        original = w.read_bytes()
        r = run(w)
        self.assertNotEqual(r.returncode, 0, "a leftover host path must fail the scrub")
        self.assertIn("file:///opt/toolchain/lib/libfoo.so", r.stdout + r.stderr)
        self.assertEqual(w.read_bytes(), original, "a refused wheel is not rewritten")


if __name__ == "__main__":
    unittest.main()
