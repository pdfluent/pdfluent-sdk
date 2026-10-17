#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The extract-js suite tests the tree build, not the registry (#636).

`run_extract_js_tests.sh` packs `crates/pdf-node` and installs the tarball
into `integrations/pdfluent-extract-js`. The package's `files` list excludes
the `.node` binary, and the per-platform optionalDependencies it would fall
back to are pinned at =1.1.0, which is not published yet -- so `index.js`
found neither the local `index.<triple>.node` nor the optional package, and
the suite failed with "Cannot find module '@pdfluent/node-linux-x64-gnu'".

Before the 1.1.0 bump it passed for the worse reason: the =1.0.0 platform
packages ARE published, so the install fetched the registry's binary and the
suite tested that instead of the tree. These cases hold the repair: the
triple is derived the way `index.js` derives it, the binary this build just
produced is placed beside the installed copy, and a missing binary fails
loudly -- while the loader still prefers that local file over any platform
package, so whatever the registry holds is never loaded. (The install stays
a plain one: --omit=optional would also drop vitest's own optional natives
and break the run.)

Static, over the script's text and the loader's: the behaviour itself is
proved by running the suite. The script cases fail against the pre-fix
script -- point RUN_EXTRACT_JS_TESTS_UNDER_TEST at it to watch them -- and
the loader case fails if a regenerated index.js stops preferring the local
file the copy relies on.
"""
from __future__ import annotations

import os
import pathlib
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = pathlib.Path(
    os.environ.get(
        "RUN_EXTRACT_JS_TESTS_UNDER_TEST",
        REPO / "scripts" / "release" / "run_extract_js_tests.sh",
    )
)
LOADER = REPO / "crates" / "pdf-node" / "index.js"


class RunExtractJsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = SCRIPT.read_text()
        cls.loader = LOADER.read_text()

    def test_tarball_install_keeps_optionals_for_the_toolchain(self):
        installs = [
            line
            for line in self.text.splitlines()
            if "npm install" in line and "TARBALL" in line
        ]
        self.assertTrue(installs, "the script no longer installs its tarball?")
        for line in installs:
            self.assertNotIn(
                "--omit=optional",
                line,
                "the tarball install must stay plain: omitting optionals "
                "also drops vitest's own optional natives (rollup's) and "
                "breaks the run. Non-reliance on the registry's binary "
                "comes from the local copy below, which the loader "
                "prefers, not from the install flags",
            )

    def test_copy_lands_after_the_install(self):
        lines = self.text.splitlines()
        installs = [i for i, l in enumerate(lines) if "TARBALL" in l and l.strip().startswith("npm install")]
        copies = [i for i, l in enumerate(lines) if "node_modules/@pdfluent/node" in l and "cp " in l]
        self.assertTrue(installs, "the script no longer installs its tarball?")
        self.assertTrue(
            copies, "the script no longer copies the tree-built binary?"
        )
        self.assertLess(
            installs[0],
            copies[0],
            "the copy must land after the install: installing over "
            "node_modules/@pdfluent/node would wipe a file placed earlier",
        )

    def test_tree_built_binary_is_placed_beside_the_installed_copy(self):
        self.assertRegex(
            self.text,
            r"crates/pdf-node/index\.\$\{TRIPLE\}\.node",
            "the script must name the binary this build produced, for the "
            "derived triple",
        )
        self.assertRegex(
            self.text,
            r"cp\s+\"?\$BUILT\"?\s+node_modules/@pdfluent/node",
            "the tree-built binary must land where the installed copy loads "
            "it from (./index.<triple>.node beside index.js)",
        )

    def test_triple_is_derived_like_the_loader_derives_it(self):
        # index.js switches on platform and arch (destructured off the
        # process) and, on linux, on the libc the process runs under. The
        # script must answer the same triple, or the copy lands under a name
        # nothing requires.
        for probe in ("process.platform", "process.arch", "glibcVersionRuntime"):
            self.assertIn(probe, self.text)
        self.assertIn(
            "const { platform, arch } = process",
            self.loader,
            "the loader no longer derives from platform and arch?",
        )
        self.assertIn(
            "glibcVersionRuntime",
            self.loader,
            "the loader no longer detects musl off the report header?",
        )

    def test_missing_binary_fails_loud(self):
        self.assertIn(
            '! -f "$BUILT"',
            self.text,
            "the script must check the build produced this platform's binary",
        )
        self.assertIn(
            "not found after npm run build",
            self.text,
            "and say so naming the file, rather than failing later inside "
            "vitest with a module the registry was supposed to provide",
        )

    def test_loader_prefers_the_local_binary(self):
        # The copy only matters because index.js requires the local file
        # first and the platform package only when it is absent. napi
        # regenerates that file on every build: if the local branch ever
        # goes, the suite silently tests the registry's binary again.
        tags = (
            "darwin-arm64",
            "darwin-x64",
            "linux-x64-gnu",
            "linux-x64-musl",
            "linux-arm64-gnu",
            "linux-arm64-musl",
            "win32-x64-msvc",
        )
        for tag in tags:
            with self.subTest(tag=tag):
                local = self.loader.find(f"index.{tag}.node")
                package = self.loader.find(f"@pdfluent/node-{tag}")
                self.assertNotEqual(local, -1, f"no local index.{tag}.node?")
                self.assertNotEqual(
                    package, -1, f"no @pdfluent/node-{tag} fallback?"
                )
                self.assertLess(
                    local,
                    package,
                    f"the local index.{tag}.node must be tried before the "
                    "platform package",
                )


if __name__ == "__main__":
    unittest.main()
