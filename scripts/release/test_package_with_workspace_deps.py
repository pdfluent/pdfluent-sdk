#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""release-smoke packages a crate together with its workspace dependencies.

`cargo package -p pdfluent` resolves the =1.1.0 workspace pins (`pdf-annot`,
...) from crates.io, where 1.1.0 does not exist yet before publication, so
any pre-publication run fails by construction (release-smoke run
37451394856). Cargo 1.94 resolves those versions locally when the crates are
selected together, so both package-validation steps in
.github/workflows/release-smoke.yml go through
scripts/release/package_with_workspace_deps.sh, which selects the crate and
its transitive workspace dependencies from
scripts/release/topological_publish_order.py --for CRATE.

This holds, at static/fixture level with no network:

  * the workflow's package steps call the helper and never a lone
    `cargo package -p <single crate>`, while the contents listing of the
    crate under test survives;
  * the helper, given `pdfluent`, selects `pdf-annot` and the rest of its
    transitive workspace dependencies, dependencies before dependents;
  * the selection is the closure, not the whole publish list: `pdf-engine`
    selects `pdf-syntax` and not `pdf-annot`;
  * `--for` itself: transitive on a fixture, dev-dependencies excluded, an
    unknown crate refused.

Each case fails when its feature is removed: the workflow scan goes red on
the old single-crate step text, the selection cases run the helper's own
--print-only, and the fixture cases drive `closure` directly.
"""
from __future__ import annotations

import importlib.util
import pathlib
import re
import shutil
import subprocess
import sys
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
WORKFLOW = REPO / ".github" / "workflows" / "release-smoke.yml"
HELPER = HERE / "package_with_workspace_deps.sh"
BASH = shutil.which("bash") or "/bin/bash"

_spec = importlib.util.spec_from_file_location(
    "topological_publish_order", HERE / "topological_publish_order.py"
)
topo = importlib.util.module_from_spec(_spec)
sys.modules["topological_publish_order"] = topo
_spec.loader.exec_module(topo)

MINIMUM_CASES = 10  # FLOOR

# The two package steps before the fix, verbatim: a lone `cargo package
# -p <crate>` per step, which resolves the =1.1.0 workspace pins from
# crates.io and fails any pre-publication run by construction.
OLD_PDFLUENT_RUN = """\
cargo package -p pdfluent --no-verify --allow-dirty
echo "--- pdfluent package contents ---"
tar -tzf target/package/pdfluent-*.crate | sort | head -60
"""
OLD_INPUT_CRATE_RUN = """\
CRATE="$INPUT_CRATE"
cargo package -p "$CRATE" --no-verify --allow-dirty
echo "--- $CRATE package contents ---"
tar -tzf "target/package/${CRATE}-"*.crate | sort | head -40
"""

_SINGLE_PACKAGE = re.compile(r"cargo\s+package\b")


def lone_packaging_runs(steps: list[dict]) -> list[str]:
    """Package steps that package without the helper. Empty means clean.

    A package step goes through package_with_workspace_deps.sh, which
    selects the crate together with its workspace dependencies; any
    `cargo package` line inside such a step is the old shape back.
    """
    found: list[str] = []
    for step in steps:
        name = str(step.get("name", ""))
        if not name.startswith("Package validation"):
            continue
        for line in str(step.get("run", "")).splitlines():
            if _SINGLE_PACKAGE.search(line):
                found.append(f"{name}: {line.strip()}")
    return found


def _package(name: str, deps: list[tuple[str, str | None]]) -> dict:
    """A `cargo metadata` package entry with just the fields `closure` reads."""
    return {
        "name": name,
        "version": "1.1.0",
        "dependencies": [{"name": d, "kind": k} for d, k in deps],
    }


def _print_only(crate: str) -> list[str]:
    proc = subprocess.run(
        [BASH, str(HELPER), crate, "--print-only"],
        capture_output=True, text=True, cwd=REPO,
    )
    assert proc.returncode == 0, proc.stderr
    return [line for line in proc.stdout.splitlines() if line]


class PackageWithWorkspaceDepsTests(unittest.TestCase):
    def test_package_steps_call_the_helper(self):
        doc = yaml.safe_load(WORKFLOW.read_text())
        steps = doc["jobs"]["smoke"]["steps"]
        package_steps = [
            s for s in steps
            if str(s.get("name", "")).startswith("Package validation")
        ]
        # A floor on what was examined: a scan over nothing is clean
        # and proves nothing.
        self.assertEqual(len(package_steps), 2)
        self.assertEqual(lone_packaging_runs(steps), [])
        for step in package_steps:
            with self.subTest(step=step["name"]):
                run = str(step.get("run", ""))
                self.assertIn("package_with_workspace_deps.sh", run)
                # The contents listing of the crate under test survives:
                # LICENSE is included, README exists, no path-only dep
                # leaks through.
                self.assertIn("tar -tzf", run)
                self.assertIn("target/package/", run)
        # ... --no-verify --allow-dirty moved into the helper, not away.
        helper = HELPER.read_text()
        self.assertIn("--no-verify", helper)
        self.assertIn("--allow-dirty", helper)

    def test_old_single_crate_step_text_is_red(self):
        found = lone_packaging_runs([
            {"name": "Package validation — pdfluent", "run": OLD_PDFLUENT_RUN},
            {"name": "Package validation — x", "run": OLD_INPUT_CRATE_RUN},
        ])
        self.assertEqual(len(found), 2)
        self.assertTrue(
            any("cargo package -p pdfluent" in line for line in found),
            found,
        )
        self.assertTrue(
            any('cargo package -p "$CRATE"' in line for line in found),
            found,
        )

    def test_helper_selects_pdfluent_with_pdf_annot(self):
        names = _print_only("pdfluent")
        self.assertIn("pdf-annot", names)
        self.assertIn("pdfluent", names)
        # The helper prints the closure the publish order computes --
        # the same source the publish uses, not a second list.
        packages = topo.workspace()
        self.assertEqual(names, topo.closure(packages, "pdfluent"))

    def test_helper_selection_is_topologically_ordered(self):
        packages = topo.workspace()
        names = _print_only("pdfluent")
        position = {name: i for i, name in enumerate(names)}
        for name in names:
            for dep in packages[name]["dependencies"]:
                if dep["name"] in position and dep["kind"] in (None, "build"):
                    self.assertLess(
                        position[dep["name"]], position[name],
                        f"{name} is packaged before its dependency {dep['name']}",
                    )
        self.assertEqual(names[-1], "pdfluent")
        self.assertGreater(len(names), 10, names)

    def test_helper_selects_the_closure_not_the_workspace(self):
        packages = topo.workspace()
        names = _print_only("pdf-engine")
        self.assertIn("pdf-syntax", names)
        self.assertNotIn(
            "pdf-annot", names,
            "pdf-annot is not a dependency of pdf-engine; selecting it "
            "would be the whole publish list, not the closure",
        )
        self.assertLess(len(names), len(topo.order(packages)))

    def test_helper_refuses_an_unknown_crate(self):
        proc = subprocess.run(
            [BASH, str(HELPER), "no-such-crate", "--print-only"],
            capture_output=True, text=True, cwd=REPO,
        )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("no-such-crate", proc.stderr)

    def test_closure_is_transitive_on_a_fixture(self):
        packages = {
            "leaf": _package("leaf", []),
            "mid": _package("mid", [("leaf", None)]),
            "top": _package("top", [("mid", None)]),
            "elsewhere": _package("elsewhere", []),
        }
        self.assertEqual(
            topo.closure(packages, "top"), ["leaf", "mid", "top"]
        )

    def test_closure_ignores_dev_dependencies(self):
        packages = {
            "a": _package("a", [("b", None), ("c", "dev")]),
            "b": _package("b", []),
            "c": _package("c", []),
        }
        self.assertEqual(topo.closure(packages, "a"), ["b", "a"])

    def test_closure_refuses_an_unknown_crate(self):
        with self.assertRaises(KeyError):
            topo.closure({"a": _package("a", [])}, "no-such-crate")

    def test_helper_passes_shellcheck(self):
        shellcheck = shutil.which("shellcheck")
        if shellcheck is None:
            self.skipTest("shellcheck is not installed here")
        proc = subprocess.run(
            [shellcheck, str(HELPER)],
            capture_output=True, text=True, cwd=REPO,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)


if __name__ == "__main__":
    unittest.main()
