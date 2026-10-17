#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The rig-text guard bites on text and filenames and passes scores.

Each case builds a throwaway runs directory and runs the guard against it
with `--root`, so nothing here touches the real tree. Both directions
matter: a guard that never goes red is decoration, and one that fires on
the score axis literally named `text` would forbid the shelf itself.

Exit codes:
  0  every case passed
  1  a case failed
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "no_rig_text_in_runs.py"

HASHED = "f8b7d66f25e95af0f4d1f7ff6f0db9d4a0ce8f7d618599d7f3d23e20b16f9f21.pdf"


def column(**extra: object) -> dict:
    col: dict = {
        "name": "PDFluent",
        "summary": {"text": {"mean": 0.6, "pages": 1}},
        "documents": [{
            "name": HASHED,
            "category": "financial_reports",
            "text": {"scored": True, "similarity": 0.6},
        }],
        "tail": {"text": [{"v": 0.6, "name": HASHED}]},
    }
    col.update(extra)
    return col


def run_file(**extra: object) -> dict:
    run: dict = {
        "run_id": "structured-doclaynet-20260101T000000Z",
        "set": "doclaynet",
        "tools": {"pdfluent": column()},
    }
    run.update(extra)
    return run


def run_guard(root: pathlib.Path) -> tuple[int, str]:
    done = subprocess.run(
        [sys.executable, str(GUARD), "--root", str(root)],
        capture_output=True, text=True, check=False)
    return done.returncode, done.stdout + done.stderr


def main() -> int:
    failures: list[str] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        if not ok:
            failures.append(f"{name}{': ' + detail if detail else ''}")

    with tempfile.TemporaryDirectory(prefix="rig-text-test-") as tmp:
        root = pathlib.Path(tmp)
        runs = root / "benchmarks" / "runs" / "structured"
        runs.mkdir(parents=True)

        def write(run: dict, name: str = "structured-doclaynet-20260101T000000Z") -> None:
            for old in runs.glob("*"):
                old.unlink()
            (runs / f"{name}.json").write_text(json.dumps(run, indent=1))

        # --- red cases: every text-bearing shape -------------------------
        run = run_file()
        run["tools"]["pdfluent"]["documents"][0]["set_in"] = [
            {"kind": "heading", "text": "NOTES"}]
        write(run)
        code, out = run_guard(root)
        check("a set_in snippet is red", code == 1, out)

        run = run_file()
        run["tools"]["pdfluent"]["documents"][0]["document"] = "Report.pdf"
        write(run)
        code, out = run_guard(root)
        check("a document field is red", code == 1, out)

        run = run_file()
        run["tools"]["pdfluent"]["documents"][0]["name"] = "PMC1-p0001.pdf"
        write(run)
        code, out = run_guard(root)
        check("a public page name is red", code == 1, out)

        run = run_file()
        run["tools"]["pdfluent"]["tail"]["text"][0]["name"] = "Report.pdf"
        write(run)
        code, out = run_guard(root)
        check("a public tail name is red", code == 1, out)

        run = run_file()
        run["tools"]["pdfluent"]["in_documents"] = {
            "documents_measured": [{"name": "Report.pdf"}]}
        write(run)
        code, out = run_guard(root)
        check("a public in-document name is red", code == 1, out)

        run = run_file()
        run["tools"]["pdfluent"]["furniture_in_the_document"] = {
            "documents_that_did_not_answer": ["Report.pdf"]}
        write(run)
        code, out = run_guard(root)
        check("a public did-not-answer entry is red", code == 1, out)

        run = run_file()
        run["tools"]["pdfluent"]["groups"] = {"table": {"per_page": {
            "table/Report_page1": {"success": True}}}}
        write(run)
        code, out = run_guard(root)
        check("a public per_page key is red", code == 1, out)

        write(run_file())
        (runs / "structured-doclaynet-20260101T000000Z.md").write_text(
            "| text | 1 | `Annual_Report.pdf` sheet-one 0.600 |\n")
        code, out = run_guard(root)
        check("a backticked filename in the .md is red", code == 1, out)

        (runs / "structured-doclaynet-20260101T000000Z.md").write_text(
            "| tables | 1 | PMC4826663-p0005 0.000 |\n")
        code, out = run_guard(root)
        check("a PMC id in the .md is red", code == 1, out)

        # --- green cases: scores, hashes and the documented skips ---------
        write(run_file())
        code, out = run_guard(root)
        check("scores keyed by hashed ids are green", code == 0, out)

        (runs / "structured-doclaynet-20260101T000000Z.md").write_text(
            "| text | 1 | f8b7d66f25e95af0… 0.600 |\n")
        code, _ = run_guard(root)
        check("a .md with short hashes only is green", code == 0)

        (runs / "paragraph-2026-09-23-589-dev-macbook-m1pro-gate.json"
         ).write_text(json.dumps({"documents": {"some_public_name.pdf": {}}}))
        code, _ = run_guard(root)
        check("a paragraph probe file is skipped", code == 0)

        empty = root / "empty"
        (empty / "benchmarks" / "runs" / "structured").mkdir(parents=True)
        code, out = run_guard(empty)
        check("an empty shelf is a loud refusal, not a pass", code == 2, out)

        code, out = run_guard(root / "missing")
        check("a missing runs directory is a loud refusal", code == 2, out)

    if failures:
        print(f"[rig-text-test] FAIL: {len(failures)} case(s):")
        for failure in failures:
            print(f"    {failure}")
        return 1
    print("[rig-text-test] OK: red on text and filenames, green on scores")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
