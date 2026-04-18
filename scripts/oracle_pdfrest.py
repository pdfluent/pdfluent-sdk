#!/usr/bin/env python3
"""pdfRest oracle: render a PDF page to PNG via the Adobe-engine pdfRest API.

Usage:
    python3 scripts/oracle_pdfrest.py --pdf /path/to/file.pdf --out /tmp/page1.png
    python3 scripts/oracle_pdfrest.py --pdf /path/to/file.pdf --out /tmp/page1.png --page 1 --dpi 150

Caches results under ~/.cache/pdfluent/pdfrest/<sha256>/<page>.png to conserve API quota.
Round-robins over the two API keys in ~/.config/pdfluent/pdfrest-keys.json.
"""
import argparse
import hashlib
import json
import os
import sys
import time
import urllib.request
import urllib.error
from pathlib import Path

API_ENDPOINT = "https://api.pdfrest.com/png"
KEYS_FILE = Path.home() / ".config" / "pdfluent" / "pdfrest-keys.json"
CACHE_DIR = Path.home() / ".cache" / "pdfluent" / "pdfrest"
DEFAULT_DPI = 150
DEFAULT_PAGE = 1

_key_index = 0


def load_keys() -> list[str]:
    if not KEYS_FILE.exists():
        sys.exit(f"ERROR: pdfRest keys file not found: {KEYS_FILE}")
    data = json.loads(KEYS_FILE.read_text())
    keys = [acc["api_key"] for acc in data.get("accounts", [])]
    if not keys:
        sys.exit("ERROR: no api_key entries found in pdfrest-keys.json")
    return keys


def next_key(keys: list[str]) -> str:
    global _key_index
    k = keys[_key_index % len(keys)]
    _key_index += 1
    return k


def pdf_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def cache_path(sha: str, page: int) -> Path:
    return CACHE_DIR / sha / f"page{page}.png"


def upload_and_render(pdf_path: Path, page: int, dpi: int, api_key: str) -> bytes:
    """Upload PDF to pdfRest /png endpoint, return PNG bytes for the requested page."""
    import urllib.parse

    boundary = "----PDFluent" + hashlib.md5(os.urandom(16)).hexdigest()

    with open(pdf_path, "rb") as fh:
        pdf_bytes = fh.read()

    # Build multipart body
    def field(name: str, value: str) -> bytes:
        return (
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="{name}"\r\n\r\n'
            f"{value}\r\n"
        ).encode()

    body = b""
    body += field("pages", str(page))
    body += field("resolution", str(dpi))
    body += (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{pdf_path.name}"\r\n'
        "Content-Type: application/pdf\r\n\r\n"
    ).encode() + pdf_bytes + b"\r\n"
    body += f"--{boundary}--\r\n".encode()

    req = urllib.request.Request(
        API_ENDPOINT,
        data=body,
        headers={
            "Api-Key": api_key,
            "Content-Type": f"multipart/form-data; boundary={boundary}",
            "Accept": "application/json",
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            payload = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body_err = e.read().decode(errors="replace")[:500]
        raise RuntimeError(f"pdfRest HTTP {e.code}: {body_err}") from e

    # Response format: outputUrl is a list ["https://..."] (one URL per page)
    raw = payload.get("outputUrl") or payload.get("outputUrls") or payload.get("output_url")
    if isinstance(raw, list):
        output_url = raw[0] if raw else None
    else:
        output_url = raw

    if not output_url:
        raise RuntimeError(f"pdfRest: no output URL in response: {json.dumps(payload)[:300]}")

    # Download the PNG
    dl_req = urllib.request.Request(output_url, headers={"Api-Key": api_key})
    with urllib.request.urlopen(dl_req, timeout=60) as resp:
        png_bytes = resp.read()

    if len(png_bytes) < 100:
        raise RuntimeError(f"pdfRest returned suspiciously small PNG ({len(png_bytes)} bytes)")

    return png_bytes


def render(pdf_path: Path, page: int, dpi: int, keys: list[str], retries: int = 2) -> Path:
    """Render a PDF page via pdfRest, using cache if available. Returns path to PNG."""
    sha = pdf_sha256(pdf_path)
    cp = cache_path(sha, page)

    if cp.exists() and cp.stat().st_size > 100:
        return cp

    cp.parent.mkdir(parents=True, exist_ok=True)
    last_err = None
    for attempt in range(retries + 1):
        key = next_key(keys)
        try:
            png_bytes = upload_and_render(pdf_path, page, dpi, key)
            cp.write_bytes(png_bytes)
            return cp
        except Exception as e:
            last_err = e
            if attempt < retries:
                time.sleep(2 ** attempt)

    raise RuntimeError(f"pdfRest render failed after {retries + 1} attempts: {last_err}")


def main():
    p = argparse.ArgumentParser(description="Render a PDF page via pdfRest (Adobe-engine oracle)")
    p.add_argument("--pdf", required=True, help="Path to PDF")
    p.add_argument("--out", required=True, help="Output PNG path")
    p.add_argument("--page", type=int, default=DEFAULT_PAGE, help="Page number (1-based)")
    p.add_argument("--dpi", type=int, default=DEFAULT_DPI, help="Render DPI")
    p.add_argument("--no-cache", action="store_true", help="Bypass cache")
    args = p.parse_args()

    pdf = Path(args.pdf)
    if not pdf.exists():
        sys.exit(f"ERROR: PDF not found: {pdf}")

    keys = load_keys()

    if args.no_cache:
        sha = pdf_sha256(pdf)
        cp = cache_path(sha, args.page)
        if cp.exists():
            cp.unlink()

    try:
        result = render(pdf, args.page, args.dpi, keys)
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        import shutil
        shutil.copy2(result, out)
        print(f"OK: {out} ({out.stat().st_size} bytes)")
    except Exception as e:
        sys.exit(f"ERROR: {e}")


if __name__ == "__main__":
    main()
