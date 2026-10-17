#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Scrub a built Python wheel before publish.

Operations:
  1. Rewrite every absolute build-host path in the SBOM (bom-ref, ref,
     dependsOn, purl download_url, ...) to its repository-relative form, so
     the artefact does not carry a build machine's layout to consumers.
  2. Rewrite the wheel `RECORD` so the new SBOM's sha256 + size match.
  3. Refuse (exit 1, naming the paths) if any absolute host path is left.

The path shapes come from the 1.1.0 CI wheels, one per runner family:

  path+file:///srv/r1/_work/engine/engine/crates/<x>#...            (linux)
  path+file:///Volumes/<vol>/.../actions-runner-mac-work/.../crates/<x>  (macOS)
  path+file:///C:/actions-runner-pdfluent/_work/.../crates/<x>      (Windows)
  ...?download_url=file://C:\\actions-runner-pdfluent\\...\\crates\\<x>  (Windows purl)

plus developer machines (/Users/..., /home/...). Workspace crates become
`file://./crates/<x>`; cargo registry and git sources (`.../registry/src/
<index>/<crate>-<ver>`, `.../git/checkouts/<repo>/<rev>`) become
`file://./cargo-registry/<crate>-<ver>` and `file://./cargo-git/<repo>/<rev>`.
A relative path is left alone, so a second run changes nothing.

Usage:
    python3 scripts/release/scrub-wheel.py <path-to-wheel>

The wheel is rewritten in place; a `.bak` is kept next to it. On a refusal
the wheel is not touched.

Tests: scripts/release/test_scrub_wheel.py.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

# The start of an absolute path: after `file://` (with the extra `/` of
# `file:///C:/`), or a bare path at the start of a string / after `=` or
# whitespace. A URL such as https://github.com/... never qualifies.
_LEAD = r"(?P<scheme>file://)/?|(?:^|(?<=[=\s]))"
# An absolute root: a POSIX `/` or a Windows drive.
_ROOT = r"(?:/|[A-Za-z]:[\\/])"
# Host directories between the root and the anchor; never a quote, space,
# `#` (purl/bom-ref fragment) or `?` (purl qualifiers).
_MID = r"[^\"\s#?]*?"
_SEP = r"[\\/]"
_SEG = r"[A-Za-z0-9_.+-]+"
_REST = rf"(?P<rest>(?:{_SEP}{_SEG})*)"

_RULES = [
    # cargo registry source: <CARGO_HOME>/registry/src/<index>/<crate>-<ver>
    (
        re.compile(rf"(?:{_LEAD}){_ROOT}{_MID}{_SEP}registry{_SEP}src{_SEP}{_SEG}{_SEP}(?P<name>{_SEG}){_REST}"),
        "cargo-registry",
    ),
    # cargo git source: <CARGO_HOME>/git/checkouts/<repo>/<rev>
    (
        re.compile(rf"(?:{_LEAD}){_ROOT}{_MID}{_SEP}git{_SEP}checkouts{_SEP}(?P<name>{_SEG}){_REST}"),
        "cargo-git",
    ),
    # workspace crate: <checkout>/crates/<name>
    (
        re.compile(rf"(?:{_LEAD}){_ROOT}{_MID}{_SEP}crates{_SEP}(?P<name>{_SEG}){_REST}"),
        "crates",
    ),
]

# What counts as an absolute host path, before and after: a non-relative
# `file://` target (cargo-cyclonedx writes the root crate as `file://.`,
# which is relative), a Windows drive path, or a bare POSIX path under a
# host root. Used for the counter and for the refusal.
HOST_PATH_RE = re.compile(
    r"file:///?(?!\.(?:/|$|[\s\"#?]))[^\s\"#?]+"
    r"|(?<![A-Za-z0-9])[A-Za-z]:[\\/][^\s\"#?]*"
    r"|(?:^|(?<=[=\s]))/(?:Users|home|srv|Volumes|private|root|tmp|var|opt|mnt|builds?|workspace|runner|__w)/[^\s\"#?]*"
)


def _relative(m: re.Match, prefix: str) -> str:
    scheme = m.group("scheme") or ""
    rest = m.group("rest").replace("\\", "/")
    return f"{scheme}./{prefix}/{m.group('name')}{rest}"


def scrub_string(s: str) -> tuple[str, int]:
    """Rewrite absolute build-host paths in `s`; return (new, rewrites)."""
    total = 0
    for rx, prefix in _RULES:
        s, n = rx.subn(lambda m, p=prefix: _relative(m, p), s)
        total += n
    return s, total


def host_paths(s: str) -> list[str]:
    return [m.group(0) for m in HOST_PATH_RE.finditer(s)]


def walk_and_scrub(node, counter: list[int]):
    """Scrub every string (values and keys) in the SBOM JSON tree."""
    if isinstance(node, dict):
        out = {}
        for k, v in node.items():
            nk, n = scrub_string(k)
            counter[0] += n
            out[nk] = walk_and_scrub(v, counter)
        return out
    if isinstance(node, list):
        return [walk_and_scrub(v, counter) for v in node]
    if isinstance(node, str):
        ns, n = scrub_string(node)
        counter[0] += n
        return ns
    return node


def b64_sha256(data: bytes) -> str:
    """RFC 7515 base64url-no-pad — the format wheel RECORD expects."""
    h = hashlib.sha256(data).digest()
    return "sha256=" + base64.urlsafe_b64encode(h).rstrip(b"=").decode("ascii")


def rewrite_record(record_text: str, target: str, new_bytes: bytes) -> str:
    """Update one line of the wheel RECORD with new sha256 + size."""
    new_hash = b64_sha256(new_bytes)
    new_size = len(new_bytes)
    out_lines = []
    for line in record_text.splitlines():
        if line.startswith(target + ","):
            out_lines.append(f"{target},{new_hash},{new_size}")
        else:
            out_lines.append(line)
    return "\n".join(out_lines) + "\n"


def scrub_wheel(wheel_path: Path) -> dict:
    """In-place scrub. Returns a summary dict; SystemExit on a leftover path."""
    summary = {
        "wheel": str(wheel_path),
        "files_rewritten": [],
        "host_paths_before": 0,
        "host_paths_rewritten": 0,
        "host_paths_after": 0,
    }

    with tempfile.TemporaryDirectory() as td:
        extracted = Path(td) / "ext"
        extracted.mkdir()

        with zipfile.ZipFile(wheel_path, "r") as z:
            z.extractall(extracted)

        dist_info_dirs = [p for p in extracted.iterdir() if p.is_dir() and p.name.endswith(".dist-info")]
        if not dist_info_dirs:
            raise SystemExit(f"no .dist-info in wheel: {wheel_path}")
        dist_info = dist_info_dirs[0]

        rewrites = []
        leftovers = []
        sbom_dir = dist_info / "sboms"
        if sbom_dir.is_dir():
            for sbom in sorted(sbom_dir.glob("*.json")):
                raw = sbom.read_text(encoding="utf-8")
                summary["host_paths_before"] += len(host_paths(raw))
                counter = [0]
                d = walk_and_scrub(json.loads(raw), counter)
                summary["host_paths_rewritten"] += counter[0]
                new_text = json.dumps(d, indent=2, ensure_ascii=False) + "\n"
                left = host_paths(new_text)
                summary["host_paths_after"] += len(left)
                rel = str(sbom.relative_to(extracted))
                leftovers += [f"{rel}: {p}" for p in left]
                rewrites.append((sbom, new_text))

        if leftovers:
            shown = "\n".join(f"  {p}" for p in leftovers[:10])
            more = f"\n  ... and {len(leftovers) - 10} more" if len(leftovers) > 10 else ""
            raise SystemExit(
                f"scrub-wheel: {len(leftovers)} absolute host path(s) left in the SBOM of "
                f"{wheel_path.name} after scrubbing; the wheel is not rewritten:\n{shown}{more}"
            )

        bak = wheel_path.with_suffix(wheel_path.suffix + ".bak")
        shutil.copyfile(wheel_path, bak)

        for sbom, new_text in rewrites:
            sbom.write_text(new_text, encoding="utf-8")
            summary["files_rewritten"].append(str(sbom.relative_to(extracted)))

        record_path = dist_info / "RECORD"
        if record_path.exists() and rewrites:
            record_text = record_path.read_text(encoding="utf-8")
            for sbom_path, new_text in rewrites:
                rel = str(sbom_path.relative_to(extracted)).replace(os.sep, "/")
                record_text = rewrite_record(record_text, rel, new_text.encode("utf-8"))
            record_path.write_text(record_text, encoding="utf-8")

        tmp_wheel = Path(td) / "out.whl"
        with zipfile.ZipFile(tmp_wheel, "w", zipfile.ZIP_DEFLATED) as zout:
            for root, _dirs, files in os.walk(extracted):
                for f in sorted(files):
                    full = Path(root) / f
                    rel = full.relative_to(extracted)
                    info = zipfile.ZipInfo(str(rel).replace(os.sep, "/"))
                    info.date_time = (1980, 1, 1, 0, 0, 0)
                    info.compress_type = zipfile.ZIP_DEFLATED
                    with open(full, "rb") as fh:
                        zout.writestr(info, fh.read())

        shutil.move(str(tmp_wheel), wheel_path)
    return summary


def main() -> None:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    wheel = Path(sys.argv[1]).resolve()
    if not wheel.is_file():
        raise SystemExit(f"not a file: {wheel}")
    summary = scrub_wheel(wheel)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
