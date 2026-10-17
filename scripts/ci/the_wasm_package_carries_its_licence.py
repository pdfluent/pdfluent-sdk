#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The npm package ships the licence file its manifest points at (#358).

WHAT BREAKS WITHOUT THIS

`@pdfluent/sdk-wasm` is not assembled from a checked-in directory. wasm-pack
emits `crates/xfa-wasm/pkg/` from the crate, `.github/workflows/wasm.yml` copies
three licence files in, and `scripts/release/transform-wasm-pkg.sh` rewrites the
manifest -- including `license: "SEE LICENSE IN LICENSE-OFFER"`, which is a
promise about a file rather than an SPDX expression, because npm's validator
rejects a `LicenseRef` compound.

That promise was false in every published version up to 1.0.0-beta.17.5. npm
does not pack the whole directory: it packs `files`, plus a short list it always
includes -- `package.json`, the README, and `LICENSE`. Not `LICENSE-OFFER`, and
not `LICENSE-COMMERCIAL`. `transform-wasm-pkg.sh` audited the three files in the
directory and never asked whether the tarball would carry them, so the tarball
went out naming a licence document nobody receives.

The transform script is the only place that decides, so this reads what it
actually writes rather than what it says: build a wasm-pack-shaped directory,
run the real script over it, and look at the manifest it leaves behind.

WHAT IS CHECKED

  the promise      when `license` reads `SEE LICENSE IN <file>`, that file is
                   in `files`
  the neighbours   every `LICENSE*` file in the directory is in `files`. The
                   offer is three documents -- the AGPL, the commercial licence
                   and the note that names both -- and a package with one of
                   them states a third of an offer
  the tarball      when `npm` is on PATH, `npm pack --dry-run` over the same
                   directory must list all three. This is the only half that
                   asks npm itself; without npm it says SKIPPED, which is not a
                   pass

WHAT IS NOT CHECKED

The real wasm build. That is `wasm-pack` plus `wasm-opt`, minutes of work for a
question about a manifest, and `scripts/release/registry_dry_run.sh` builds the
real package and checks the real tarball.

Usage:
    python3 scripts/ci/the_wasm_package_carries_its_licence.py

Exit codes:
    0  the manifest's promise is one the tarball keeps
    1  it is not
"""
from __future__ import annotations

import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
TRANSFORM = REPO / "scripts/release/transform-wasm-pkg.sh"

# The three documents the offer is made of. LICENSE and LICENSE-COMMERCIAL are
# the texts; LICENSE-OFFER is the note that names both and is what the npm
# manifest points at.
LICENCE_FILES = ("LICENSE", "LICENSE-COMMERCIAL", "LICENSE-OFFER")

# What wasm-pack emits: the crate's name, and a `files` list that names the
# build outputs and nothing else. Everything the package needs beyond that is
# something a later step has to put there.
WASM_PACK_MANIFEST = {
    "name": "xfa-wasm",
    "type": "module",
    "version": "0.0.0",
    "files": ["xfa_wasm_bg.wasm", "xfa_wasm.js", "xfa_wasm.d.ts"],
    "main": "xfa_wasm.js",
    "types": "xfa_wasm.d.ts",
    "sideEffects": ["./snippets/*"],
}


def fixture(root: pathlib.Path) -> pathlib.Path:
    """A directory shaped like the one wasm.yml hands the transform script."""
    pkg = root / "pkg"
    (pkg / "snippets" / "xfa-wasm-0000").mkdir(parents=True)
    (pkg / "snippets" / "xfa-wasm-0000" / "inline0.js").write_text("export {}\n")
    (pkg / "package.json").write_text(json.dumps(WASM_PACK_MANIFEST, indent=2) + "\n")
    for name in ("xfa_wasm_bg.wasm", "xfa_wasm.js", "xfa_wasm.d.ts",
                 "xfa_wasm_bg.wasm.d.ts", "README.md"):
        (pkg / name).write_text("placeholder\n")
    # The three real texts: the transform script reads them, so a stub with the
    # wrong words would fail its audit instead of this guard's check.
    for name in LICENCE_FILES:
        shutil.copyfile(REPO / name, pkg / name)
    return pkg


def judge(pkg: pathlib.Path) -> list[str]:
    """Every way the manifest in this directory promises what it cannot keep."""
    findings: list[str] = []
    manifest = json.loads((pkg / "package.json").read_text(encoding="utf-8"))
    listed = set(manifest.get("files") or [])

    promised = re.fullmatch(r"SEE LICENSE IN (\S+)", str(manifest.get("license", "")))
    if promised and promised.group(1) not in listed:
        findings.append(
            f'license is "{manifest["license"]}" but {promised.group(1)} is not in '
            f"files, so npm does not pack it: the published manifest names a "
            f"document nobody receives"
        )

    for name in LICENCE_FILES:
        if (pkg / name).is_file() and name not in listed:
            findings.append(
                f"{name} is in the package directory but not in files; the offer "
                f"is three documents and this tarball would carry fewer"
            )
    return findings


def packed(pkg: pathlib.Path) -> set[str] | None:
    """What npm would actually put in the tarball, or None without npm."""
    if not shutil.which("npm"):
        return None
    done = subprocess.run(["npm", "pack", "--dry-run", "--json"],
                          cwd=pkg, capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"npm pack --dry-run failed: {done.stderr[-400:]}")
    return {entry["path"] for entry in json.loads(done.stdout)[0]["files"]}


def main() -> int:
    if not TRANSFORM.is_file():
        print(f"[wasm-licence] FAIL: {TRANSFORM} is missing; nothing writes the "
              "published manifest and this guard cannot read it", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory() as directory:
        pkg = fixture(pathlib.Path(directory))
        done = subprocess.run(["bash", str(TRANSFORM), str(pkg), "1.0.0"],
                              capture_output=True, text=True)
        if done.returncode != 0:
            print("[wasm-licence] FAIL: transform-wasm-pkg.sh refused the "
                  "reference package directory", file=sys.stderr)
            print(done.stdout[-2000:] + done.stderr[-2000:], file=sys.stderr)
            return 1

        findings = judge(pkg)

        try:
            inside = packed(pkg)
        except RuntimeError as why:
            print(f"[wasm-licence] FAIL: {why}", file=sys.stderr)
            return 1
        if inside is None:
            print("[wasm-licence]   SKIPPED (not a pass): npm is not on PATH, so "
                  "the tarball half of this guard did not run")
        else:
            for name in LICENCE_FILES:
                if name not in inside:
                    findings.append(
                        f"npm pack leaves {name} out of the tarball, whatever the "
                        f"manifest says"
                    )

    if findings:
        print("[wasm-licence] FAIL: the sdk-wasm package would not ship the "
              "licence its manifest names", file=sys.stderr)
        for line in findings:
            print(f"  - {line}", file=sys.stderr)
        return 1

    print("[wasm-licence] OK: the manifest names LICENSE-OFFER and the tarball "
          "carries all three licence documents")
    return 0


if __name__ == "__main__":
    sys.exit(main())
