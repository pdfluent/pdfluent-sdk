#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The error-catalogue sync check still refuses a Node class nobody exports.

`scripts/release/error_catalogue_sync.sh` reads the Node column of
docs/error_catalogue.md and requires every `Pdfluent*` name in it to be
declared in crates/pdf-node/index.d.ts and exported by the typed-error layer.
That column named five classes that never existed for as long as it had been
there (#587), so this runs the check on a copy of the tree three ways:

  1. the tree as it is                          -> passes
  2. a Node cell naming PdfluentNonsenseError   -> fails, and names it
  3. the typed-error layer without its export   -> fails, and names PdfluentError

Exit codes:
  0  the check passes the real tree and refuses both mutations
  1  it does not
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
FILES = [
    "scripts/release/error_catalogue_sync.sh",
    "crates/pdfluent/src/error.rs",
    "docs/error_catalogue.md",
    "crates/pdf-node/index.d.ts",
    "crates/pdf-node/scripts/build/typed-error-layer.js",
]


def copy_tree(dest: pathlib.Path) -> None:
    for rel in FILES:
        target = dest / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO / rel, target)


def run(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", str(root / "scripts/release/error_catalogue_sync.sh")],
        capture_output=True,
        text=True,
        check=False,
    )


def rename_first_node_cell(catalogue: pathlib.Path, name: str) -> None:
    lines = catalogue.read_text(encoding="utf-8").splitlines(keepends=True)
    header = None
    for i, line in enumerate(lines):
        cells = line.rstrip("\n").split(" | ")
        if header is None and "Node" in [c.strip() for c in cells]:
            header = [c.strip() for c in cells]
            continue
        if header is not None and line.startswith("| `E-"):
            column = header.index("Node")
            cells[column] = name
            lines[i] = " | ".join(cells) + "\n"
            catalogue.write_text("".join(lines), encoding="utf-8")
            return
    raise SystemExit("no catalogue row with a Node column found")


def main() -> int:
    failures: list[str] = []

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        copy_tree(root)
        result = run(root)
        if result.returncode != 0:
            failures.append(f"the real tree fails the check:\n{result.stderr}")

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        copy_tree(root)
        rename_first_node_cell(root / "docs/error_catalogue.md", "PdfluentNonsenseError")
        result = run(root)
        if result.returncode == 0 or "PdfluentNonsenseError" not in result.stderr:
            failures.append("a Node cell naming PdfluentNonsenseError passed the check")

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        copy_tree(root)
        layer = root / "crates/pdf-node/scripts/build/typed-error-layer.js"
        text = layer.read_text(encoding="utf-8")
        stripped = re.sub(r"^module\.exports\.PdfluentError = .*$", "", text, flags=re.M)
        if stripped == text:
            failures.append("the typed-error layer has no `module.exports.PdfluentError =` line")
        layer.write_text(stripped, encoding="utf-8")
        result = run(root)
        if result.returncode == 0 or "PdfluentError" not in result.stderr:
            failures.append("a layer that does not export PdfluentError passed the check")

    if failures:
        for f in failures:
            print(f"FAIL: {f}", file=sys.stderr)
        return 1
    print("error_catalogue_sync.sh passes the tree and refuses both phantom mutations.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
