#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A skip must stay a skip, on both sides of the exit code.

Two halves have to agree for the SSIM step to report honestly, and until
07-09-2026 they did not. The presence check said SKIPPED (not a pass) and exited
3; the workflow step ran it under `set -euo pipefail` and turned that into a red
job. Every wave was red on a corpus that is not on the machine, in the colour a
real SSIM regression would have used (#361).

So this test covers both halves, and neither by paraphrase:

  * the check itself -- a missing document and a document with the right name
    and the wrong bytes are 3 and say so on stderr; a manifest that cannot be
    read is 2, because that is a defect in the tree and not a state of the
    runner; a complete corpus is 0;

  * the mapping in the workflow -- the step's own `run` body is lifted out of
    `ci-ephemeral.yml` and executed against a stubbed `python3`, so that
    "3 becomes a notice and a green step, anything else stays red" is proved by
    running it rather than by reading it.
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts/ci/ssim_gate_corpus_is_present.py"
WORKFLOW = ROOT / ".github/workflows/ci-ephemeral.yml"

failures: list[str] = []


def fail(line: str) -> None:
    failures.append(line)


def manifest(tmp: pathlib.Path, documents: dict[str, bytes], *, pinned: list[str] | None = None,
             sha_of: dict[str, bytes] | None = None) -> pathlib.Path:
    """A manifest naming `pinned`, with `documents` actually written to disk.

    `sha_of` lets a document be written with one content and pinned to the
    hash of another -- the right name, the wrong year.
    """
    pinned = list(documents) if pinned is None else pinned
    sha_of = sha_of or {}
    for naam, inhoud in documents.items():
        pad = tmp / "corpus" / naam
        pad.parent.mkdir(parents=True, exist_ok=True)
        pad.write_bytes(inhoud)
    entries = [
        {
            "file": naam,
            "source": f"corpus/{naam}",
            "sha256": hashlib.sha256(sha_of.get(naam, documents.get(naam, b""))).hexdigest(),
        }
        for naam in pinned
    ]
    pad = tmp / "SSIM_GATE_MANIFEST.json"
    pad.write_text(json.dumps({"entries": entries}))
    return pad


def run_check(cwd: pathlib.Path, manifest_pad: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(CHECK), "--manifest", str(manifest_pad)],
        cwd=cwd, capture_output=True, text=True,
    )


def step_body() -> str | None:
    """The shipped text of the SSIM step, found by its subject.

    Keyed on the script it calls rather than on the step's name: a renamed step
    that still runs the check is the same step, and a test that missed it would
    report a green repository while the mapping had gone.
    """
    doc = yaml.safe_load(WORKFLOW.read_text())
    bodies = [
        stap["run"]
        for job in doc.get("jobs", {}).values()
        for stap in (job.get("steps", []) or [])
        if "ssim_gate_corpus_is_present.py" in (stap.get("run") or "")
    ]
    if len(bodies) > 1:
        fail(f"{len(bodies)} steps call the presence check; this test reads one of "
             "them and would miss a regression in the others")
    return bodies[0] if bodies else None


def run_body(lichaam: str, stub_rc: int) -> subprocess.CompletedProcess:
    """The step body, with `python3` and `cargo` replaced by stubs.

    The stub answers `stub_rc` for the presence check and 0 for everything
    after it, so what is measured here is the step's own arithmetic on the exit
    code and nothing else.
    """
    with tempfile.TemporaryDirectory() as tmp:
        base = pathlib.Path(tmp)
        bin_dir = base / "bin"
        bin_dir.mkdir()
        (bin_dir / "python3").write_text(
            "#!/usr/bin/env bash\n"
            'case "$*" in *ssim_gate_corpus_is_present.py*) exit ' f"{stub_rc}" ";; esac\n"
            "exit 0\n"
        )
        (bin_dir / "cargo").write_text("#!/usr/bin/env bash\nexit 0\n")
        for stub in bin_dir.iterdir():
            stub.chmod(0o755)
        omgeving = {
            "PATH": f"{bin_dir}:/usr/bin:/bin",
            "RUNNER_TEMP": str(base),
            "HOME": str(base),
        }
        return subprocess.run(
            ["bash", "-c", lichaam], cwd=base, env=omgeving,
            capture_output=True, text=True,
        )


def main() -> int:
    if not CHECK.is_file():
        print(f"[ssim-skip] FAIL: {CHECK.relative_to(ROOT)} is gone")
        return 1

    with tempfile.TemporaryDirectory() as tmp:
        base = pathlib.Path(tmp)

        # 1. A complete corpus runs the gate. Without this the red cases below
        #    would prove only that the check can answer 3, not that it tells
        #    the two situations apart.
        heel = manifest(base / "heel", {"f1040.pdf": b"%PDF-1.4 a", "f1096.pdf": b"%PDF-1.4 b"})
        out = run_check(base / "heel", heel)
        if out.returncode != 0:
            fail(f"a complete corpus did not pass (rc={out.returncode}): {out.stderr.strip()[:300]}")

        # 2. One document missing is a skip, not a failure, and it says which.
        weg = manifest(base / "weg", {"f1040.pdf": b"%PDF-1.4 a"},
                       pinned=["f1040.pdf", "f1096.pdf"])
        out = run_check(base / "weg", weg)
        if out.returncode != 3:
            fail(f"a missing document answered {out.returncode}, not 3; a skip that "
                 "returns 1 is indistinguishable from a regression")
        if "SKIPPED (not a pass)" not in out.stderr:
            fail("the missing-document message does not say SKIPPED (not a pass) on "
                 f"stderr: {out.stderr.strip()[:200]}")
        if "f1096.pdf" not in out.stderr:
            fail("the missing-document message does not name the document that is gone")

        # 3. The right name with the wrong bytes. This is the case a file-count
        #    check passes and the thresholds cannot survive: IRS and USCIS
        #    reissue their forms every year.
        fout = manifest(base / "fout", {"f1040.pdf": b"%PDF-1.4 2027 edition"},
                        sha_of={"f1040.pdf": b"%PDF-1.4 2026 edition"})
        out = run_check(base / "fout", fout)
        if out.returncode != 3:
            fail(f"a document with the right name and the wrong bytes answered "
                 f"{out.returncode}, not 3")
        elif "f1040.pdf" not in out.stderr:
            fail("the wrong-bytes message does not name the document")

        # 4. An empty manifest must not read as "all present". This is the
        #    empty-glob shape: nothing checked, everything approved.
        leeg = base / "leeg"
        leeg.mkdir()
        (leeg / "m.json").write_text(json.dumps({"entries": []}))
        out = run_check(leeg, leeg / "m.json")
        if out.returncode == 0:
            fail("a manifest naming no documents passed; an empty gate is not a "
                 "complete corpus")

        # 5. A manifest that cannot be read is a defect in the tree, not a
        #    state of the runner, so it must NOT be mapped to a notice.
        out = run_check(leeg, leeg / "does-not-exist.json")
        if out.returncode in (0, 3):
            fail(f"an unreadable manifest answered {out.returncode}; that would be "
                 "reported as a skip and the gate would vanish silently")

    # 6. The mapping in the workflow, executed.
    lichaam = step_body()
    if lichaam is None:
        print("[ssim-skip] FAIL: no step in ci-ephemeral.yml calls "
              "ssim_gate_corpus_is_present.py; the gate reported a red job on a "
              "missing corpus for three months without one")
        return 1

    out = run_body(lichaam, 3)
    if out.returncode != 0:
        fail(f"the step is red on exit 3 (rc={out.returncode}); a skip is not a "
             f"failure: {(out.stderr or out.stdout).strip()[:300]}")
    elif "::notice::" not in out.stdout:
        fail("the step passes on exit 3 without writing a notice, so the skip is "
             "invisible in the run summary")
    elif "SKIPPED (not a pass)" not in out.stdout:
        fail("the notice does not carry SKIPPED (not a pass), which is the wording "
             "CLAUDE.md requires for a measurement that was not made")

    out = run_body(lichaam, 1)
    if out.returncode == 0:
        fail("the step is green on exit 1; mapping 3 to a notice must not swallow a "
             "real SSIM regression")

    out = run_body(lichaam, 2)
    if out.returncode == 0:
        fail("the step is green on exit 2, which is the presence check saying its "
             "own manifest is unreadable")

    out = run_body(lichaam, 0)
    if out.returncode != 0:
        fail(f"the step is red on exit 0, so a runner that HAS the fifty could not "
             f"run the gate: {(out.stderr or out.stdout).strip()[:300]}")

    if failures:
        print("[ssim-skip] FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("[ssim-skip] OK: a missing or reissued document is 3 and says SKIPPED (not "
          "a pass); an unreadable manifest is not; and the step maps 3 to a notice "
          "while 1 and 2 stay red.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
