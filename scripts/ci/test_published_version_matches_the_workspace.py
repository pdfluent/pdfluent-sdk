#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A version already on crates.io must be the content standing here (#358).

The guard under test is `scripts/release/published_version_matches_the_workspace.py`.
Measured on the 1.0.0 publish of 16-09-2026: five forks carried local changes
under a version number crates.io had already served, `pdfluent-jbig2` could not
be packaged because of it, and for the crates whose API had not moved the
publish would have succeeded and shipped the old content.

A stub registry rather than crates.io: the case that matters is a published
crate whose bytes differ, and waiting for the real registry to hold one of those
is not a test. `--registry-base` is the seam.
"""
from __future__ import annotations

import http.server
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import threading

GUARD = (pathlib.Path(__file__).resolve().parents[1]
         / "release" / "published_version_matches_the_workspace.py")

MINIMUM_CASES = 7  # FLOOR

NAME = "pdfluent-ccitt"
VERSION = "0.2.2"


def crate_tarball(files: dict[str, bytes]) -> bytes:
    """A .crate the way cargo writes one: everything under <name>-<version>/."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for rel, body in files.items():
            info = tarfile.TarInfo(f"{NAME}-{VERSION}/{rel}")
            info.size = len(body)
            archive.addfile(info, io.BytesIO(body))
    return buffer.getvalue()


class Registry(http.server.BaseHTTPRequestHandler):
    """Serves one crate. `tarball = None` means the version is not published."""

    tarball: bytes | None = None

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler's name
        if self.path == f"/{NAME}/{VERSION}":
            if self.tarball is None:
                self.send_error(404)
                return
            body = json.dumps({"version": {"num": VERSION}}).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif self.path == f"/{NAME}/{VERSION}/download":
            if self.tarball is None:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Length", str(len(self.tarball)))
            self.end_headers()
            self.wfile.write(self.tarball)
        else:
            self.send_error(500)

    def log_message(self, *_args: object) -> None:
        return


def serve(tarball: bytes | None):
    handler = type("Bound", (Registry,), {"tarball": tarball})
    server = http.server.HTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def run(base: str, package_dir: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD), "--name", NAME, "--version", VERSION,
         "--dir", str(package_dir), "--registry-base", base],
        capture_output=True, text=True)


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    published = {
        "Cargo.toml": b'[package]\nname = "pdfluent-ccitt"\n',       # cargo's rewrite
        "Cargo.toml.orig": b'[package]\nname = "pdfluent-ccitt"\nversion = "0.2.2"\n',
        ".cargo_vcs_info.json": b'{"git":{"sha1":"deadbeef"}}\n',
        "src/lib.rs": b"pub fn push_pixels(count: u32) {}\n",
        "README.md": b"# pdfluent-ccitt\n",
    }

    with tempfile.TemporaryDirectory() as tmp:
        local = pathlib.Path(tmp) / "hayro-ccitt"
        (local / "src").mkdir(parents=True)
        (local / "Cargo.toml").write_bytes(published["Cargo.toml.orig"])
        (local / "src" / "lib.rs").write_bytes(published["src/lib.rs"])
        (local / "README.md").write_bytes(published["README.md"])

        # 1. The version is not on the registry at all: nothing to compare.
        with serve(None) as server:
            base = f"http://127.0.0.1:{server.server_port}"
            r = run(base, local)
            case("an unpublished version passes", r.returncode == 0,
                 r.stdout + r.stderr)

        with serve(crate_tarball(published)) as server:
            base = f"http://127.0.0.1:{server.server_port}"

            # 2. Published and identical.
            r = run(base, local)
            case("a published version with the same content passes",
                 r.returncode == 0, r.stdout + r.stderr)

            # 3. What cargo generates is not evidence: the normalised Cargo.toml
            #    and .cargo_vcs_info.json are not in the workspace at all, and a
            #    guard that read them would refuse every crate ever published.
            case("cargo's own files are not counted as drift",
                 r.returncode == 0 and "Cargo.toml:" not in r.stdout
                 and "cargo_vcs_info" not in r.stdout, r.stdout)

            # 4. THE DEFECT: source changed under a version already served.
            (local / "src" / "lib.rs").write_bytes(
                b"pub fn push_pixels(white: bool, count: u32) {}\n")
            r = run(base, local)
            case("a source file that changed refuses",
                 r.returncode == 1 and "src/lib.rs" in r.stdout, r.stdout + r.stderr)
            (local / "src" / "lib.rs").write_bytes(published["src/lib.rs"])

            # 5. And the manifest, which is how a dependency bump gets noticed.
            (local / "Cargo.toml").write_bytes(
                b'[package]\nname = "pdfluent-ccitt"\nversion = "0.2.2"\n'
                b'[dependencies]\nserde = "1"\n')
            r = run(base, local)
            case("a manifest that changed refuses",
                 r.returncode == 1 and "Cargo.toml.orig" in r.stdout,
                 r.stdout + r.stderr)
            (local / "Cargo.toml").write_bytes(published["Cargo.toml.orig"])

            # 6. A published file deleted locally is drift too, and it is the
            #    half a one-way content comparison would have missed.
            (local / "README.md").unlink()
            r = run(base, local)
            case("a published file that is gone locally refuses",
                 r.returncode == 1 and "README.md" in r.stdout,
                 r.stdout + r.stderr)
            (local / "README.md").write_bytes(published["README.md"])

            # 7. And the other direction, which a comparison that walks only the
            #    tarball cannot see at all: a file ADDED since. There is no
            #    archive member to visit, every published file still matches,
            #    and the crate would read as identical while publishing it
            #    produced different content. (codex, #1917)
            (local / "tests").mkdir()
            (local / "tests" / "new_case.rs").write_bytes(b"#[test]\nfn t() {}\n")
            r = run(base, local)
            case("a file added since the publish refuses",
                 r.returncode == 1 and "tests/new_case.rs" in r.stdout,
                 r.stdout + r.stderr)
            (local / "tests" / "new_case.rs").unlink()
            (local / "tests").rmdir()

        # 7. A registry that cannot be read is not a verdict: exit 2, not 0
        #    and not 1. A guard that answered "fine" here would go quiet at
        #    exactly the moment nobody is watching.
        server = serve(None)
        port = server.server_port
        server.shutdown()
        server.server_close()
        r = run(f"http://127.0.0.1:{port}", local)
        case("an unreachable registry is not a pass and not a verdict",
             r.returncode == 2, f"rc={r.returncode} {r.stdout}{r.stderr}")

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: a published version must be the content standing here")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
