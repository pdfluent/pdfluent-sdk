#!/usr/bin/env python3
"""Scrub a built Python wheel before publish.

Operations:
  1. Rewrite any absolute build-tree path that appears in the SBOM
     (bom-ref, ref, purl download_url, etc.) so the artefact does not
     carry the developer's `$HOME` to consumers.
  2. Rewrite the wheel `RECORD` so the new SBOM's sha256 + size match.

Usage:
    python3 scripts/release/scrub-wheel.py <path-to-wheel>

The wheel is rewritten in place; a `.bak` is kept next to it.
"""

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


# Catch absolute paths that point into a crates/ subdirectory:
#   /Users/.../crates/<name>       (macOS dev)
#   /home/.../crates/<name>        (Linux dev / CI)
#   /private/var/.../crates/<name> (some macOS sandboxes)
ABS_TO_CRATES_RE = re.compile(
    r"(file://)?/(?:Users|home|private|root|workspace|builds?)/[^\"#?\s]*?/(crates/[A-Za-z0-9_-]+)"
)


def scrub_string(s: str) -> str:
    """Replace absolute build-tree paths with `./crates/<name>` relative form."""
    return ABS_TO_CRATES_RE.sub(lambda m: f"{m.group(1) or ''}./{m.group(2)}", s)


def walk_and_scrub(node):
    """Recursively scrub any string inside the SBOM JSON tree."""
    if isinstance(node, dict):
        for k, v in list(node.items()):
            if isinstance(v, str):
                node[k] = scrub_string(v)
            else:
                walk_and_scrub(v)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            if isinstance(v, str):
                node[i] = scrub_string(v)
            else:
                walk_and_scrub(v)


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
    """In-place scrub. Returns a summary dict."""
    bak = wheel_path.with_suffix(wheel_path.suffix + ".bak")
    shutil.copyfile(wheel_path, bak)

    summary = {"wheel": str(wheel_path), "files_rewritten": [], "user_paths_before": 0, "user_paths_after": 0}

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
        sbom_dir = dist_info / "sboms"
        if sbom_dir.is_dir():
            for sbom in sbom_dir.glob("*.json"):
                raw = sbom.read_text(encoding="utf-8")
                summary["user_paths_before"] += len(re.findall(r"/Users/|/home/", raw))
                d = json.loads(raw)
                walk_and_scrub(d)
                new_text = json.dumps(d, indent=2, ensure_ascii=False) + "\n"
                leaks_after = len(re.findall(r"/Users/|/home/", new_text))
                summary["user_paths_after"] += leaks_after
                if leaks_after:
                    # Hard fail — we expect zero leaks.
                    raise SystemExit(
                        f"SBOM still contains user-home path after scrub: {sbom}\n"
                        f"  remaining matches: {leaks_after}"
                    )
                sbom.write_text(new_text, encoding="utf-8")
                rewrites.append((sbom, new_text.encode("utf-8")))
                summary["files_rewritten"].append(str(sbom.relative_to(extracted)))

        record_path = dist_info / "RECORD"
        if record_path.exists() and rewrites:
            record_text = record_path.read_text(encoding="utf-8")
            for sbom_path, new_bytes in rewrites:
                rel = str(sbom_path.relative_to(extracted))
                record_text = rewrite_record(record_text, rel, new_bytes)
            record_path.write_text(record_text, encoding="utf-8")

        tmp_wheel = Path(td) / "out.whl"
        with zipfile.ZipFile(tmp_wheel, "w", zipfile.ZIP_DEFLATED) as zout:
            for root, _dirs, files in os.walk(extracted):
                for f in sorted(files):
                    full = Path(root) / f
                    rel = full.relative_to(extracted)
                    info = zipfile.ZipInfo(str(rel))
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
