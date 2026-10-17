#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A published crate's content cannot move without CI noticing (#358).

The guard under test is `--against-record` of
`scripts/release/published_version_matches_the_workspace.py`. On 14-10-2026 a
trial commit changed crates/lopdf -- four source files and a new test -- under
`pdfluent-lopdf 0.39.5`, which crates.io had served since 16-09. Every pipeline
after it was green: the only comparison with the registry ran at the release,
and the 1.1.0 dry run found it two days later. Case 2 below is that commit's
shape, against a fixture record.

No network: the record is built from a fixture tarball with the guard's own
`record_entry`, and the comparison is the offline one CI runs.
"""
from __future__ import annotations

import importlib.util
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

GUARD = (pathlib.Path(__file__).resolve().parents[1]
         / "release" / "published_version_matches_the_workspace.py")

MINIMUM_CASES = 14  # FLOOR

NAME = "pdfluent-fork"
VERSION = "0.39.5"

ROOT_MANIFEST = (b'[workspace]\nmembers = ["crates/fork"]\n'
                 b'[workspace.package]\nedition = "2021"\n'
                 b'[workspace.dependencies]\nserde = "1"\n')
CRATE_MANIFEST = (b'[package]\nname = "pdfluent-fork"\nversion = "0.39.5"\n'
                  b'edition.workspace = true\nexclude = ["benches/"]\n'
                  b'[dependencies]\nserde = { workspace = true }\n')

# What the published tarball holds, and what sits beside it in the crate
# directory without shipping: a nested fuzz package and an excluded directory.
PUBLISHED = {
    "Cargo.toml": b'[package]\nname = "pdfluent-fork"\n',  # cargo's rewrite
    "Cargo.toml.orig": CRATE_MANIFEST,
    ".cargo_vcs_info.json": b'{"git":{"sha1":"deadbeef"}}\n',
    "src/lib.rs": b"pub fn trailer() {}\n",
    "src/reader.rs": b"pub fn read() {}\n",
    "README.md": b"# fork\n",
}
UNSHIPPED = {
    "fuzz/Cargo.toml": b'[package]\nname = "fork-fuzz"\nversion = "0.0.0"\n',
    "fuzz/src/main.rs": b"fn main() {}\n",
    "benches/big.rs": b"// a benchmark that never ships\n",
}


def load_guard():
    spec = importlib.util.spec_from_file_location("guard", GUARD)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def crate_tarball(files: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for rel, body in files.items():
            info = tarfile.TarInfo(f"{NAME}-{VERSION}/{rel}")
            info.size = len(body)
            archive.addfile(info, io.BytesIO(body))
    return buffer.getvalue()


def git(repo: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", "-c", "user.name=fixture", "-c",
                    "user.email=fixture@example.invalid", "-C", str(repo), *args],
                   check=True, capture_output=True, env=sealed_env(cwd=repo))


def write(path: pathlib.Path, body: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body)


def run(repo: pathlib.Path, record: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD), "--against-record", "--repo", str(repo),
         "--record-file", str(record)],
        capture_output=True, text=True, env=sealed_env(cwd=repo))


def main() -> int:
    guard = load_guard()
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as tmp:
        repo = pathlib.Path(tmp) / "repo"
        crate = repo / "crates" / "fork"
        write(repo / "Cargo.toml", ROOT_MANIFEST)
        write(crate / "Cargo.toml", CRATE_MANIFEST)
        for rel, body in PUBLISHED.items():
            if rel not in guard.GENERATED and rel != guard.ORIGINAL_MANIFEST:
                write(crate / rel, body)
        for rel, body in UNSHIPPED.items():
            write(crate / rel, body)
        git(repo, "init", "-q")
        git(repo, "add", ".")

        record = pathlib.Path(tmp) / "record.json"
        entry = guard.record_entry(repo, NAME, VERSION, crate,
                                   crate_tarball(PUBLISHED))
        guard.write_record(record, [entry])

        def restore() -> None:
            write(repo / "Cargo.toml", ROOT_MANIFEST)
            write(crate / "Cargo.toml", CRATE_MANIFEST)
            for rel in ("src/lib.rs", "src/reader.rs", "README.md"):
                write(crate / rel, PUBLISHED[rel])
            git(repo, "add", "-A", ".")

        # 1. The record holds what cargo would not ship, and nothing of cargo's.
        case("the record leaves out the nested package, keeps the excluded file",
             entry["unshipped"] == ["benches/big.rs"]
             and "Cargo.toml" not in entry["files"]
             and ".cargo_vcs_info.json" not in entry["files"]
             and "Cargo.toml.orig" in entry["files"], json.dumps(entry))

        r = run(repo, record)
        case("an unchanged tree passes", r.returncode == 0, r.stdout + r.stderr)

        # 2. THE DEFECT of 14-10-2026: source changed, version unchanged.
        write(crate / "src" / "reader.rs", b"pub fn read() { inherit_trailer_keys(); }\n")
        r = run(repo, record)
        case("a source file changed under the published version refuses",
             r.returncode == 1 and "src/reader.rs: published and local differ" in r.stdout,
             r.stdout + r.stderr)
        restore()

        # 3. A file added since -- the new test file of that same commit.
        write(crate / "tests" / "xref_stream_trailer_keys.rs", b"#[test]\nfn t() {}\n")
        git(repo, "add", ".")
        r = run(repo, record)
        case("a tracked file added since the publish refuses",
             r.returncode == 1 and "tests/xref_stream_trailer_keys.rs" in r.stdout,
             r.stdout + r.stderr)
        git(repo, "rm", "-q", "--cached", "crates/fork/tests/xref_stream_trailer_keys.rs")
        (crate / "tests" / "xref_stream_trailer_keys.rs").unlink()
        (crate / "tests").rmdir()

        # 4. A published file gone locally.
        (crate / "README.md").unlink()
        r = run(repo, record)
        case("a published file that is gone locally refuses",
             r.returncode == 1 and "README.md: published, and not in" in r.stdout,
             r.stdout + r.stderr)
        restore()

        # 5. The manifest, compared as Cargo.toml.orig.
        write(crate / "Cargo.toml", CRATE_MANIFEST + b'log = "0.4"\n')
        r = run(repo, record)
        case("a manifest that changed refuses",
             r.returncode == 1 and "Cargo.toml.orig" in r.stdout, r.stdout + r.stderr)
        restore()

        # 6. A root workspace value the crate inherits: no file of the crate
        #    changes, and its published Cargo.toml would.
        write(repo / "Cargo.toml", ROOT_MANIFEST.replace(b'serde = "1"', b'serde = "1.0.228"'))
        r = run(repo, record)
        case("an inherited workspace dependency that changed refuses",
             r.returncode == 1 and "dependencies.serde" in r.stdout, r.stdout + r.stderr)
        restore()

        # 7. The repair: the version moves, so the old record no longer binds.
        write(crate / "Cargo.toml", CRATE_MANIFEST.replace(b'"0.39.5"', b'"0.39.6"'))
        write(crate / "src" / "reader.rs", b"pub fn read() { inherit_trailer_keys(); }\n")
        r = run(repo, record)
        case("a bumped version with changed content passes",
             r.returncode == 0 and "moved on" in r.stdout, r.stdout + r.stderr)
        restore()

        # 8. A file in the nested fuzz package does not ship.
        write(crate / "fuzz" / "src" / "extra.rs", b"// fuzz\n")
        git(repo, "add", ".")
        r = run(repo, record)
        case("a new file in a nested package passes", r.returncode == 0,
             r.stdout + r.stderr)

        # 9. Nor does a new file under an `exclude`d directory, or a change to
        #    a file the record already lists as unshipped.
        write(crate / "benches" / "other.rs", b"// another\n")
        write(crate / "benches" / "big.rs", b"// changed, still unshipped\n")
        git(repo, "add", ".")
        r = run(repo, record)
        case("excluded and unshipped files pass", r.returncode == 0,
             r.stdout + r.stderr)

        # 10. A file on disk that git does not track is not in the commit CI
        #     sees, and the comparison is of the commit.
        write(crate / "src" / "scratch.rs", b"// local only\n")
        r = run(repo, record)
        case("an untracked file is not counted", r.returncode == 0,
             r.stdout + r.stderr)
        (crate / "src" / "scratch.rs").unlink()

        # 11. No record, or an empty one, is not a pass.
        r = run(repo, pathlib.Path(tmp) / "missing.json")
        case("a missing record is exit 2", r.returncode == 2, f"rc={r.returncode} {r.stderr}")
        empty = pathlib.Path(tmp) / "empty.json"
        guard.write_record(empty, [])
        r = run(repo, empty)
        case("an empty record is exit 2, said as a skip",
             r.returncode == 2 and "SKIPPED (not a pass)" in r.stderr,
             f"rc={r.returncode} {r.stderr}")

        # 12. A record that points at the wrong directory cannot compare.
        moved = pathlib.Path(tmp) / "moved.json"
        guard.write_record(moved, [dict(entry, dir="crates/elsewhere")])
        r = run(repo, moved)
        case("a recorded directory that is not there is exit 2",
             r.returncode == 2, f"rc={r.returncode} {r.stdout}{r.stderr}")

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: a published crate's content cannot move under its version unseen")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
