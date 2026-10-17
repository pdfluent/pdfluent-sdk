#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No file a crate publishes names a path only this repository has (#358).

`prepublish_crate_audit.sh` refuses such a file, and that is the right place for
it to be refused -- but it finds out one crate at a time, during the publish,
after the crates before it are already on crates.io and cannot be taken back.
The 1.0.0 publish stopped at crate 8 of 32 on a single line in `pdf-syntax`:

    let path = "../../corpus/f3800.pdf";

a `#[ignore]`d benchmark reading a document out of the repository's corpus
directory, in a file that ships. Three more crates shipped their whole `tests/`
and `examples/` trees, whose fixtures live three directories up and are not in
the tarball at all: 30 lines over four crates, none of which a reader could use
and all of which describe a tree they do not have.

WHAT IT READS. `cargo package --list` for every publishable workspace crate --
the file set cargo would upload, so `include` and `exclude` are honoured and a
test that does not travel is not judged -- and then the same
LEAKAGE_PATH_PATTERNS the prepublish audit uses, imported from it rather than
copied, so the two cannot drift apart.

WHAT IT CANNOT SEE. Anything the audit looks for that is not a path: secrets,
oversized files, the licence files. Those stay with the audit, which reads the
built tarball and can weigh what this one only lists.

EXIT CODES

  0  no packaged file names an internal path
  1  one does, and the output names the crate, the file and the line
  2  it could not tell: no cargo, or a tree too dirty for `cargo package --list`
     (which refuses uncommitted changes) -- not a verdict, and not a pass
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve()
REPO = HERE.parents[2]
AUDIT = REPO / "scripts" / "release" / "audit_package_tree.py"

# Bytes, not text. Reading a font or a PDF as UTF-8 finds nothing and costs
# time; the audit reads the real tarball and judges those on their own terms.
BINARY = {".png", ".jpg", ".jpeg", ".pdf", ".brotli", ".bin", ".ttf", ".otf",
          ".icc", ".zip", ".gz", ".afm", ".woff", ".woff2", ".ico"}


def patterns() -> list:
    """The prepublish audit's path rules, imported so there is one copy."""
    spec = importlib.util.spec_from_file_location("audit_package_tree", AUDIT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {AUDIT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["audit_package_tree"] = module  # dataclasses needs to find it
    spec.loader.exec_module(module)
    return module.LEAKAGE_PATH_PATTERNS


def findings(package_dir: pathlib.Path, files: list[str],
             rules: list) -> list[tuple[str, int, str]]:
    """(file, line number, the line) for every packaged line naming such a path."""
    found: list[tuple[str, int, str]] = []
    for rel in files:
        path = package_dir / rel
        if not path.is_file() or path.suffix.lower() in BINARY:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if any(rule.search(line) for rule in rules):
                found.append((rel, number, line.strip()))
                break  # one per file is enough to name it
    return found


def publishable(repo: pathlib.Path) -> list[tuple[str, pathlib.Path]]:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=str(repo)))
    return sorted((p["name"], pathlib.Path(p["manifest_path"]).parent)
                  for p in meta["packages"] if p.get("publish") != [])


def packaged(repo: pathlib.Path, name: str) -> list[str] | None:
    out = subprocess.run(["cargo", "package", "--list", "-p", name],
                         cwd=str(repo), capture_output=True, text=True)
    if out.returncode != 0:
        return None
    return [line.strip() for line in out.stdout.splitlines() if line.strip()]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--crate", action="append",
                    help="only this crate; repeatable (default: all publishable)")
    args = ap.parse_args()

    rules = patterns()
    crates = publishable(REPO)
    if args.crate:
        wanted = set(args.crate)
        crates = [c for c in crates if c[0] in wanted]
        missing = wanted - {c[0] for c in crates}
        if missing:
            print(f"SKIPPED (not a pass): no publishable crate named "
                  f"{', '.join(sorted(missing))}", file=sys.stderr)
            return 2

    bad: list[str] = []
    unreadable: list[str] = []
    for name, package_dir in crates:
        files = packaged(REPO, name)
        if files is None:
            unreadable.append(name)
            print(f"  ??  {name} — `cargo package --list` would not say what ships")
            continue
        found = findings(package_dir, files, rules)
        if not found:
            print(f"  ok  {name} — {len(files)} file(s)")
            continue
        for rel, number, line in found:
            bad.append(f"{name}  {rel}:{number}  {line[:100]}")
            print(f"  NO  {name}  {rel}:{number}  {line[:100]}")

    if bad:
        print("", file=sys.stderr)
        print("[packaged-paths] a published file names a path only this repository "
              "has. Keep the file out of the package (`exclude`/`include` in its "
              "Cargo.toml), or build the path from CARGO_MANIFEST_DIR instead of "
              "writing it out:", file=sys.stderr)
        for line in bad:
            print(f"  {line}", file=sys.stderr)
        return 1
    if unreadable:
        print("", file=sys.stderr)
        print("[packaged-paths] SKIPPED (not a pass): `cargo package --list` "
              "failed for " + ", ".join(unreadable) + ". It refuses a package "
              "directory with uncommitted changes, so this says nothing about "
              "those crates.", file=sys.stderr)
        return 2
    print(f"[packaged-paths] OK: {len(crates)} crate(s), no packaged file names "
          "an internal path")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
