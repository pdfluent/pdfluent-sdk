#!/usr/bin/env python3
"""Prepare the CI corpus sample from a manifest.

Downloads or copies PDFs listed in CI_CORPUS_MANIFEST.json into the target directory.
The manifest contains file metadata and a download URL or local path.

Usage:
    python3 scripts/prepare_ci_corpus.py \
        --manifest corpus/CI_CORPUS_MANIFEST.json \
        --out corpus/ci-sample-200
"""
import argparse
import hashlib
import json
import shutil
import sys
import urllib.request
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--manifest", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--skip-verify", action="store_true")
    args = p.parse_args()

    manifest_path = Path(args.manifest)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if not manifest_path.exists():
        print(f"ERROR: manifest not found: {manifest_path}", file=sys.stderr)
        print("Run scripts/create_ci_manifest.py to generate it.", file=sys.stderr)
        sys.exit(1)

    manifest = json.loads(manifest_path.read_text())
    entries = manifest.get("entries", [])
    print(f"Preparing CI corpus: {len(entries)} entries → {out_dir}")

    ok = 0
    skip = 0
    errors = 0

    for entry in entries:
        fname = entry["file"]
        out_path = out_dir / fname

        if out_path.exists():
            if not args.skip_verify and "sha256" in entry:
                if sha256_file(out_path) != entry["sha256"]:
                    print(f"  WARN: {fname} checksum mismatch, re-downloading")
                    out_path.unlink()
                else:
                    skip += 1
                    continue
            else:
                skip += 1
                continue

        source = entry.get("source")
        if not source:
            print(f"  SKIP: {fname} (no source in manifest)")
            errors += 1
            continue

        try:
            if source.startswith("http"):
                urllib.request.urlretrieve(source, out_path)
            else:
                src = Path(source)
                if src.exists():
                    shutil.copy2(src, out_path)
                else:
                    print(f"  ERROR: source not found: {source}")
                    errors += 1
                    continue
            ok += 1
        except Exception as e:
            print(f"  ERROR: {fname}: {e}")
            errors += 1

    present = len(list(out_dir.glob("*.pdf")))
    print(f"Done: {present} PDFs in corpus (copied={ok}, cached={skip}, errors={errors})")

    if errors > len(entries) * 0.1:
        print(f"ERROR: too many failures ({errors}/{len(entries)})", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
