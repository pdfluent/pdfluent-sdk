#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""zip_cli_stage.py — zip a staged CLI package with forward-slash entries.

The Windows CLI archives are zips of the stage dir's contents (no top dir).
PowerShell 5.1's Compress-Archive writes backslash entry names
(man\\man1\\pdfluent.1), which the artefact gate refuses; zipfile writes
forward slashes on every pool, so release-binaries.yml cuts both Windows
zips through this helper (build 2).

Usage: python3 scripts/release/zip_cli_stage.py STAGE_DIR OUT.zip
"""
from __future__ import annotations

import sys
import zipfile
from pathlib import Path


def zip_stage(stage: Path, out: Path) -> list[str]:
    """Zip the stage dir's contents (no top dir); return the entry names."""
    names: list[str] = []
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(stage.rglob("*")):
            if path.is_dir():
                continue
            arc = path.relative_to(stage).as_posix()
            zf.write(path, arc)
            names.append(arc)
    return names


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: zip_cli_stage.py STAGE_DIR OUT.zip", file=sys.stderr)
        return 2
    stage, out = Path(argv[1]), Path(argv[2])
    if not stage.is_dir():
        print(f"zip_cli_stage: no stage dir: {stage}", file=sys.stderr)
        return 1
    names = zip_stage(stage, out)
    if not names:
        print(f"zip_cli_stage: nothing staged in {stage}", file=sys.stderr)
        return 1
    print(f"zip_cli_stage: {len(names)} entries -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
