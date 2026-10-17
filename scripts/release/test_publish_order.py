#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The crates.io publish order puts every dependency before its dependent.

Every intra-workspace dependency is pinned `=<version>`, and the verification
build of `cargo package` resolves those against the registry, not the path.
So publishing `pdfluent-cli` before `pdfluent` 1.1.0 is on crates.io fails --
measured on m116: `cargo publish -p pdfluent-cli --dry-run` gave 17 errors
against registry 1.0.0. The order in `topological_publish_order.py` is what
stands between that failure and a half-published release, and this holds it:

  * on this tree: `pdfluent` comes before `pdfluent-cli`, and every
    publishable crate comes before each crate that depends on it;
  * on fixtures: the ordering itself -- normal and build dependencies count,
    dev-dependencies do not, a cycle is an error, ties break alphabetically;
  * the checker used for the first point goes red on a swapped order, so a
    regression is a named failure rather than a silent pass.
"""
from __future__ import annotations

import importlib.util
import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location(
    "topological_publish_order", HERE / "topological_publish_order.py"
)
topo = importlib.util.module_from_spec(_spec)
sys.modules["topological_publish_order"] = topo
_spec.loader.exec_module(topo)

MINIMUM_CASES = 7  # FLOOR


def _package(name: str, deps: list[tuple[str, str | None]]) -> dict:
    """A `cargo metadata` package entry with just the fields `order` reads."""
    return {
        "name": name,
        "version": "1.1.0",
        "dependencies": [{"name": d, "kind": k} for d, k in deps],
    }


def violations(packages: dict[str, dict], ordered: list[str]) -> list[str]:
    """Every dependency that its dependent precedes. Empty means topological.

    The same edge rule as `topological_publish_order.order`: normal and build
    dependencies count, dev-dependencies do not -- cargo strips them from the
    uploaded manifest, so they impose no publish order.
    """
    position = {name: i for i, name in enumerate(ordered)}
    found: list[str] = []
    for name in ordered:
        for dep in packages[name]["dependencies"]:
            if dep["name"] not in position or dep["kind"] not in (None, "build"):
                continue
            if position[dep["name"]] > position[name]:
                found.append(f"{name} is published before its dependency {dep['name']}")
    return found


class PublishOrderTests(unittest.TestCase):
    def test_facade_before_cli_on_this_tree(self):
        packages = topo.workspace()
        self.assertIn("pdfluent", packages)
        self.assertIn("pdfluent-cli", packages)
        ordered = topo.order(packages)
        self.assertLess(
            ordered.index("pdfluent"),
            ordered.index("pdfluent-cli"),
            "pdfluent-cli verifies against the registry's pdfluent; "
            "publishing it first fails (m116: 17 errors against 1.0.0)",
        )

    def test_every_dependency_before_its_dependent_on_this_tree(self):
        packages = topo.workspace()
        ordered = topo.order(packages)
        self.assertEqual(violations(packages, ordered), [])
        # A floor on what was examined: an order over nothing is topological
        # and proves nothing.
        self.assertGreater(len(ordered), 10, ordered)

    def test_swapped_cli_and_facade_is_red(self):
        packages = topo.workspace()
        ordered = topo.order(packages)
        swapped = list(ordered)
        i, j = swapped.index("pdfluent"), swapped.index("pdfluent-cli")
        swapped[i], swapped[j] = swapped[j], swapped[i]
        found = violations(packages, swapped)
        self.assertIn(
            "pdfluent-cli is published before its dependency pdfluent", found
        )

    def test_dependencies_first_on_a_fixture(self):
        packages = {
            "leaf": _package("leaf", []),
            "mid": _package("mid", [("leaf", None)]),
            "top": _package("top", [("mid", None), ("leaf", None)]),
        }
        ordered = topo.order(packages)
        self.assertLess(ordered.index("leaf"), ordered.index("mid"))
        self.assertLess(ordered.index("mid"), ordered.index("top"))
        self.assertEqual(violations(packages, ordered), [])

    def test_build_dependencies_count_dev_dependencies_do_not(self):
        packages = {
            "a": _package("a", [("b", "build")]),
            "b": _package("b", []),
            "c": _package("c", [("d", "dev")]),
            "d": _package("d", [("c", "dev")]),
        }
        ordered = topo.order(packages)
        self.assertLess(ordered.index("b"), ordered.index("a"))
        # The dev-dependency cycle between c and d is not in the published
        # graph, so it orders instead of raising.
        self.assertEqual(violations(packages, ordered), [])

    def test_ties_break_alphabetically(self):
        packages = {
            "b-crate": _package("b-crate", []),
            "a-crate": _package("a-crate", []),
        }
        self.assertEqual(topo.order(packages), ["a-crate", "b-crate"])

    def test_a_normal_dependency_cycle_is_an_error(self):
        packages = {
            "a": _package("a", [("b", None)]),
            "b": _package("b", [("a", None)]),
        }
        with self.assertRaises(ValueError):
            topo.order(packages)


if __name__ == "__main__":
    unittest.main()
