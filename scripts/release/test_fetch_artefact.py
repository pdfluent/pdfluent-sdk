#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Release downloads survive a flaky network and refuse a wrong artefact.

15-10: `curl exit 56` and ENOTFOUND on the Mac runner failed a smoke as if
the product had failed. scripts/release/fetch_artefact.sh is the one fetcher
both release-natives.yml and node-bindings.yml use: it retries transient
network errors a bounded number of times, verifies each zip against the
artifact API's `digest`, and a download that still fails exits non-zero
naming the ENVIRONMENT.

Every case runs against a fake curl on PATH that fails or answers on cue.
The workflow cases run the workflows' own shell functions, so a workflow
that goes back to a bare `curl -fsSL` fails here.
"""
from __future__ import annotations

import hashlib
import io
import json
import os
import pathlib
import re
import subprocess
import tempfile
import unittest
import zipfile

REPO = pathlib.Path(__file__).resolve().parents[2]
HELPER = REPO / "scripts" / "release" / "fetch_artefact.sh"
NATIVES = REPO / ".github" / "workflows" / "release-natives.yml"
NODE = REPO / ".github" / "workflows" / "node-bindings.yml"

ENV_MSG = "::error::environment: artefact download failed"

# The fake curl: records each call, fails the first $FAIL_TIMES calls with
# $FAIL_RC, then answers by route. Options that take a value are skipped with
# their value, so the URL is the one positional argument.
FAKE_CURL = r'''#!/bin/sh
out=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2;;
    -H|--retry|--retry-delay|--connect-timeout|--max-time) shift 2;;
    -*) shift;;
    *) url="$1"; shift;;
  esac
done
n=$(cat "$FAKE_DIR/count" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$FAKE_DIR/count"
printf '%s\n' "$url" >> "$FAKE_DIR/urls"
if [ "$n" -le "${FAIL_TIMES:-0}" ]; then
  # A reset mid-transfer leaves a partial file behind, as real curl does.
  printf 'partial' > "$out"
  exit "${FAIL_RC:-56}"
fi
case "$url" in
  */runs\?*) cp "$FAKE_DIR/runs.json" "$out";;
  */artifacts\?*) cp "$FAKE_DIR/artifacts.json" "$out";;
  */zip) cp "$FAKE_DIR/artefact.zip" "$out";;
  *) exit 22;;
esac
'''


def _zip_bytes() -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("pdfluent-9.9.9-py3-none-any.whl", "wheel")
    return buf.getvalue()


class _Fake:
    def __init__(self, root: pathlib.Path, digest: str | None = None):
        self.root = root
        self.zip = _zip_bytes()
        self.digest = digest if digest is not None else "sha256:" + hashlib.sha256(self.zip).hexdigest()
        (root / "artefact.zip").write_bytes(self.zip)
        (root / "runs.json").write_text(json.dumps({"workflow_runs": [{"id": 4242}]}))
        (root / "artifacts.json").write_text(json.dumps({"artifacts": [
            {"id": 7, "name": "pdf-node-release-x", "digest": self.digest},
            {"id": 8, "name": "wheels-linux-x86_64", "digest": self.digest},
        ]}))
        bindir = root / "bin"
        bindir.mkdir()
        (bindir / "curl").write_text(FAKE_CURL)
        (bindir / "curl").chmod(0o755)
        self.bindir = bindir

    def env(self, **extra: str) -> dict[str, str]:
        env = dict(os.environ)
        env.update(
            PATH=str(self.bindir) + os.pathsep + os.environ["PATH"],
            FAKE_DIR=str(self.root), RUNNER_TEMP=str(self.root),
            GH_TOKEN="t", GITHUB_API_URL="https://api.invalid", GITHUB_REPOSITORY="o/r",
            FETCH_PAUSE="0", FETCH_RETRY_DELAY="0",
        )
        env.update(extra)
        return env

    def calls(self) -> int:
        p = self.root / "count"
        return int(p.read_text()) if p.exists() else 0

    def helper(self, *args: str, **extra: str) -> subprocess.CompletedProcess:
        return subprocess.run(["bash", str(HELPER), *args], capture_output=True, text=True,
                              timeout=60, cwd=REPO, env=self.env(**extra))


class FetchHelperTests(unittest.TestCase):
    def test_transient_failures_then_success_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            out = fake.root / "a.zip"
            proc = fake.helper("zip", "7", "pdf-node-release-x", fake.digest, str(out),
                               FAIL_TIMES="2", FAIL_RC="56")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertEqual(out.read_bytes(), fake.zip)
            self.assertEqual(fake.calls(), 3)

    def test_dns_failure_then_success_on_the_api_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            out = fake.root / "runs.out"
            proc = fake.helper("api", "actions/workflows/x.yml/runs?per_page=1", str(out),
                               FAIL_TIMES="1", FAIL_RC="6")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertEqual(json.loads(out.read_text())["workflow_runs"][0]["id"], 4242)

    def test_always_failing_download_fails_naming_the_environment(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            out = fake.root / "a.zip"
            proc = fake.helper("zip", "7", "pdf-node-release-x", fake.digest, str(out),
                               FAIL_TIMES="99", FAIL_RC="56")
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn(ENV_MSG + " after retries", proc.stderr)
            self.assertIn("pdf-node-release-x", proc.stderr)
            self.assertEqual(fake.calls(), 3, "retries must be bounded")
            self.assertFalse(out.exists(), "a failed download must leave no file to use")

    def test_http_error_is_not_retried_and_names_the_environment(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            proc = fake.helper("api", "nowhere", str(fake.root / "x"), FAIL_TIMES="99", FAIL_RC="22")
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn(ENV_MSG, proc.stderr)
            self.assertIn("not retried", proc.stderr)
            self.assertEqual(fake.calls(), 1)

    def test_digest_mismatch_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp), digest="sha256:" + "0" * 64)
            out = fake.root / "a.zip"
            proc = fake.helper("zip", "7", "pdf-node-release-x", fake.digest, str(out))
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("digest mismatch", proc.stderr)
            self.assertFalse(out.exists(), "a mismatching zip must not be handed on")

    def test_missing_digest_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            out = fake.root / "a.zip"
            proc = fake.helper("zip", "7", "pdf-node-release-x", "-", str(out))
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("no sha256 digest", proc.stderr)
            self.assertFalse(out.exists())


def _block(text: str, start: str, end: str) -> str:
    i = text.index(start)
    j = text.index(end, i)
    return "\n".join(line[10:] if line.startswith(" " * 10) else line
                     for line in text[i:j].splitlines())


class WorkflowsUseTheFetcherTests(unittest.TestCase):
    def test_no_bare_curl_in_the_download_steps(self):
        for wf in (NATIVES, NODE):
            with self.subTest(workflow=wf.name):
                code = "\n".join(l.split("#", 1)[0] for l in wf.read_text().splitlines())
                m = re.search(r".*\bcurl\s+-.*", code)
                self.assertIsNone(m, f"{wf.name} calls curl directly, use fetch_artefact.sh: {m and m.group(0).strip()}")
                self.assertIn("scripts/release/fetch_artefact.sh", code)

    def test_release_natives_download_survives_a_transient_reset(self):
        fns = _block(NATIVES.read_text(), "          api_to() {", "          NODE_RUN=")
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            script = ("set -euo pipefail\nPY=python3\n" + fns +
                      '\nr="$(newest_successful_run node-bindings.yml)"\n'
                      'download_matching "$r" "pdf-node-release-" "$FAKE_DIR/arts"\n')
            proc = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=60,
                                  cwd=REPO, env=fake.env(FAIL_TIMES="1", FAIL_RC="56", REF="b"))
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertTrue((fake.root / "arts" / "pdf-node-release-x").is_dir())
            # The run id survived the reset: a fetch that failed silently
            # left it empty and listed `actions/runs//artifacts` instead.
            self.assertIn("/actions/runs/4242/artifacts", (fake.root / "urls").read_text())

    def test_release_natives_run_lookup_failure_stops_naming_the_environment(self):
        # errexit is off inside $(...): the run lookup must still stop the step.
        fns = _block(NATIVES.read_text(), "          api_to() {", "          NODE_RUN=")
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            script = ("set -euo pipefail\nPY=python3\n" + fns +
                      '\nr="$(newest_successful_run node-bindings.yml)"\necho "REACHED [$r]"\n')
            proc = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=60,
                                  cwd=REPO, env=fake.env(FAIL_TIMES="99", FAIL_RC="6", REF="b"))
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertNotIn("REACHED", proc.stdout)
            self.assertIn(ENV_MSG, proc.stderr)

    def test_release_natives_refuses_a_mismatching_artefact(self):
        fns = _block(NATIVES.read_text(), "          api_to() {", "          NODE_RUN=")
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp), digest="sha256:" + "1" * 64)
            script = ("set -euo pipefail\nPY=python3\n" + fns +
                      '\ndownload_matching 4242 "pdf-node-release-" "$FAKE_DIR/arts"\n')
            proc = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=60,
                                  cwd=REPO, env=fake.env(REF="b"))
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("digest mismatch", proc.stderr)
            self.assertFalse((fake.root / "arts").exists())

    def test_node_parity_wheel_download_survives_a_transient_reset(self):
        text = NODE.read_text()
        step = _block(text, "          set -euo pipefail\n          PY=\"$(command -v python3",
                      "          ls \"$WHEELS\"")
        with tempfile.TemporaryDirectory() as tmp:
            fake = _Fake(pathlib.Path(tmp))
            cwd = REPO / "crates" / "pdf-node"
            proc = subprocess.run(["bash", "-c", step + '\nls "$WHEELS"\n'], capture_output=True,
                                  text=True, timeout=60, cwd=cwd,
                                  env=fake.env(FAIL_TIMES="2", FAIL_RC="56", SHA="abc",
                                               WHEEL_ARTIFACT="wheels-linux-x86_64", RELEASE_LANE="1"))
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("pdfluent-9.9.9-py3-none-any.whl", proc.stdout)


if __name__ == "__main__":
    unittest.main()
