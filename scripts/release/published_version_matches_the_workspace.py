#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A version already on crates.io must be the content standing here (#358).

crates.io is append-only: a version that exists can never be replaced. So a
workspace crate whose version is already published and whose files have moved on
is in one of two states, and both are bad:

* the API diverged -- every dependent fails `cargo package --verify`, because
  the verification build resolves the dependency from the registry and not from
  the path. Measured on 16-09-2026, at crate 6 of 32 of the 1.0.0 publish:
  `pdfluent-jbig2` would not build against the `pdfluent-ccitt 0.2.2` the
  registry serves, while it builds against the `0.2.2` in this workspace.

* the API did not diverge -- and then it is worse, because the publish
  SUCCEEDS. What ships resolves the old content out of the registry, so a user
  runs code nobody here measured, and nothing anywhere says so.

Five forks were in that state that day: `pdf-syntax 0.5.6`, `pdf-interpret
0.5.8`, `pdfluent-ccitt 0.2.2`, `pdf-font 1.0.0-beta.5` (all three with real
code differences) and `pdfluent-cff 0.2.1` (a test module). The repair is a
version bump; this is what notices that a bump is due, before a publish rather
than during one.

THE RECORD, AND WHY IT EXISTS (16-10-2026). Asking crates.io needs the network,
so the comparison ran in exactly one place: publish_ordered.sh, at the release.
CI ran only the stub-registry test of the logic. A trial commit changed
crates/lopdf under the published 0.39.5 on 14-10-2026, every pipeline after it
was green, and the 1.1.0 dry run was the first thing to notice -- two days
later, on the morning of the release. The check was right and stood in the one
spot where its answer arrives last.

So the registry's answer is written down. `--record` stores, for every
workspace crate whose version is published and identical, the SHA-256 of every
file in the published tarball, the files git tracks in that crate that the
tarball left out, and the workspace values the crate inherits (which land in
cargo's normalised Cargo.toml without touching any file of the crate itself).
`--against-record` compares the tree with that, offline, on every push: the
commit that changes a published crate's content without moving its version is
the commit that goes red. The live `--all` still runs at the release, and it
refuses a record that is missing a published version, so the record cannot fall
behind the registry unnoticed.

    published_version_matches_the_workspace.py --all
    published_version_matches_the_workspace.py --all --record
    published_version_matches_the_workspace.py --against-record
    published_version_matches_the_workspace.py --name pdf-syntax \\
        --version 0.5.6 --dir crates/pdf-syntax

Exit codes: 0 every crate is either unpublished or identical, 1 at least one
version is published with different content (or, with --all, the record lacks a
published version), 2 the registry, the record or the tree could not be read
(which is not a verdict, and does not pretend to be one).
"""
from __future__ import annotations

import argparse
import fnmatch
import hashlib
import io
import os
import json
import pathlib
import subprocess
import sys
import tarfile
import tomllib
import urllib.error
import urllib.request

# What cargo itself writes into the tarball, so it is not evidence of drift:
# `Cargo.toml` is cargo's normalised rewrite of the manifest (path dependencies
# stripped), `Cargo.lock` is added for binaries, and `.cargo_vcs_info.json`
# records the commit it was packaged from. `Cargo.toml.orig` is the manifest as
# written and IS compared -- against the local `Cargo.toml`, which is the file
# it is a copy of.
GENERATED = {"Cargo.toml", "Cargo.lock", ".cargo_vcs_info.json"}
ORIGINAL_MANIFEST = "Cargo.toml.orig"

# The same base, and the same environment override, as
# `crate_version_is_on_crates_io.py`: one publish run should not be able to ask
# two different registries, and a test that redirects one redirects both.
DEFAULT_BASE = "https://crates.io/api/v1/crates"
BASE_ENV = "PDFLUENT_CRATES_IO_API"
TIMEOUT = 60

RECORD_NAME = pathlib.Path("scripts") / "release" / "published_crate_contents.json"
RECORD_SCHEMA = 1

# The git variables that steer path resolution. A hook exports GIT_DIR, and
# `git ls-files` under it would list another tree than the one asked about.
GIT_STEERING = ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX",
                "GIT_COMMON_DIR")


class Unreadable(Exception):
    """The registry, the record or the tree did not answer. Not a verdict."""


def _get(url: str) -> bytes | None:
    """Body, or None for a 404. Anything else raises Unreadable."""
    try:
        with urllib.request.urlopen(url, timeout=TIMEOUT) as response:
            return response.read()
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return None
        raise Unreadable(f"{url}: HTTP {exc.code}") from exc
    except Exception as exc:  # noqa: BLE001 - urllib raises a wide family here
        raise Unreadable(f"{url}: {exc}") from exc


def published_tarball(base: str, name: str, version: str) -> bytes | None:
    """The .crate bytes for name@version, or None if that version is not there."""
    meta = _get(f"{base}/{name}/{version}")
    if meta is None:
        return None
    body = _get(f"{base}/{name}/{version}/download")
    if body is None:
        raise Unreadable(f"{name}@{version} is in the index but has no tarball")
    return body


def packaged_now(repo: pathlib.Path, name: str) -> set[str] | None:
    """What `cargo publish` would put in the tarball today, as relative paths.

    None when cargo cannot answer -- no cargo, or a manifest it will not read.
    The caller then falls back to walking the package directory, and says so.
    """
    out = subprocess.run(["cargo", "package", "--list", "-p", name],
                         cwd=str(repo), capture_output=True, text=True)
    if out.returncode != 0:
        return None
    return {line.strip() for line in out.stdout.splitlines() if line.strip()}


def walked(package_dir: pathlib.Path) -> set[str]:
    """Every file under the package directory, as `cargo package --list` names them.

    Only for `--name/--version/--dir`, where there is no workspace to ask cargo
    about. It is an approximation and not cargo's answer: a manifest with
    `include` or `exclude` ships fewer files than the directory holds --
    `crates/pdf-forms` leaves out `examples/` and `tests/` that way -- so a walk
    would call an unchanged crate drifted. Where cargo CAN be asked and does not
    answer, that is "could not tell" and not this. (codex, #1917)

    `target/` and dotted directories are cargo's build output and its own
    bookkeeping, and a crate ships neither.
    """
    found: set[str] = set()
    for path in package_dir.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(package_dir)
        if rel.parts and (rel.parts[0] == "target" or rel.parts[0].startswith(".")):
            continue
        found.add(str(rel))
    return found


def tarball_files(tarball: bytes, name: str, version: str) -> dict[str, bytes]:
    """Every regular file in a .crate, keyed by its path inside the package."""
    root = f"{name}-{version}/"
    files: dict[str, bytes] = {}
    with tarfile.open(fileobj=io.BytesIO(tarball), mode="r:gz") as archive:
        for member in archive.getmembers():
            if not member.isfile() or not member.name.startswith(root):
                continue
            handle = archive.extractfile(member)
            files[member.name[len(root):]] = handle.read() if handle else b""
    return files


def differences(tarball: bytes, name: str, version: str,
                package_dir: pathlib.Path,
                local_files: set[str] | None = None) -> list[str]:
    """Every file that says the published version is not what is standing here.

    BOTH DIRECTIONS. Walking only the tarball misses a file that has been ADDED
    since -- a new test, an example, an asset -- because there is no archive
    member to visit, and the crate would then report identical while publishing
    would produce different content. (codex, #1917)
    """
    found: list[str] = []
    published = tarball_files(tarball, name, version)
    for rel, body in published.items():
        if rel in GENERATED:
            continue
        local_rel = "Cargo.toml" if rel == ORIGINAL_MANIFEST else rel
        local = package_dir / local_rel
        if not local.is_file():
            found.append(f"{rel}: published, and not in the workspace")
        elif local.read_bytes() != body:
            found.append(f"{rel}: published and local differ")

    for rel in sorted((local_files or set()) - set(published) - GENERATED
                      - {ORIGINAL_MANIFEST}):
        found.append(f"{rel}: in the workspace, and not in the published crate")
    return found


def workspace_crates(repo: pathlib.Path) -> list[tuple[str, str, str | None]]:
    """Every publishable workspace package: name, version, manifest path.

    The manifest path can be absent -- `cargo metadata` always carries it, a
    stub standing in for cargo in a test need not -- and it is only needed for
    a version that turns out to be published, so it stays a string here and is
    resolved in `check`.
    """
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=str(repo)))
    return [(p["name"], p["version"], p.get("manifest_path"))
            for p in meta["packages"] if p.get("publish") != []]


def check(base: str, name: str, version: str,
          package_dir: pathlib.Path | None,
          repo: pathlib.Path | None = None) -> tuple[str, list[str], bytes | None]:
    """('unpublished' | 'same' | 'drifted', the differing files, the tarball).

    The registry is asked first: an unpublished version has nothing to compare
    against, so it does not need to know where the crate lives -- and
    `cargo package --list`, which is the expensive half, is not run for it.
    """
    tarball = published_tarball(base, name, version)
    if tarball is None:
        return "unpublished", [], None
    if package_dir is None:
        raise Unreadable(f"{name}@{version} is published, and cargo metadata "
                         "gave no manifest_path to compare it with")
    if repo is not None:
        local_files = packaged_now(repo, name)
        if local_files is None:
            raise Unreadable(
                f"{name}@{version} is published, and `cargo package --list` "
                "would not say what ships today. Walking the directory instead "
                "would call a crate with `include` or `exclude` drifted when it "
                "is not.")
    else:
        local_files = walked(package_dir)
    found = differences(tarball, name, version, package_dir, local_files)
    return ("drifted" if found else "same"), found, tarball


# --- the record --------------------------------------------------------------

def _sha256(body: bytes) -> str:
    return hashlib.sha256(body).hexdigest()


def _toml(path: pathlib.Path) -> dict:
    try:
        with path.open("rb") as fh:
            return tomllib.load(fh)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise Unreadable(f"{path}: {exc}") from exc


def git_tracked(repo: pathlib.Path, rel_dir: str) -> set[str]:
    """The files git tracks under rel_dir, relative to it."""
    env = {k: v for k, v in os.environ.items() if k not in GIT_STEERING}
    out = subprocess.run(
        ["git", "-c", "safe.directory=*", "-C", str(repo), "ls-files", "-z",
         "--", rel_dir],
        capture_output=True, env=env)
    if out.returncode != 0:
        raise Unreadable(f"git ls-files {rel_dir}: "
                         f"{out.stderr.decode(errors='replace').strip()}")
    prefix = rel_dir.rstrip("/") + "/"
    return {p[len(prefix):] for p in out.stdout.decode().split("\0")
            if p.startswith(prefix)}


def nested_package_dirs(files: set[str]) -> set[str]:
    """Subdirectories holding their own Cargo.toml. cargo never ships those."""
    return {f.rsplit("/", 1)[0] + "/" for f in files
            if f.endswith("/Cargo.toml")}


def _under(rel: str, dirs: set[str]) -> bool:
    return any(rel.startswith(d) for d in dirs)


def _pattern_hits(pattern: str, rel: str) -> bool:
    """A manifest include/exclude pattern against a path, conservatively.

    cargo reads these with gitignore rules. This answers only the shapes the
    rules agree on -- a slash-free pattern matches any path component, a
    pattern with a slash matches from the package root, a trailing slash or
    `/**` takes the directory -- because a false "excluded" here would hide a
    file that ships. A pattern it cannot place matches nothing, and the file
    is then reported: a loud wrong answer, never a quiet one.
    """
    p = pattern.lstrip("/")
    if p.endswith("/**"):
        p = p[:-3] + "/"
    if p.endswith("/"):
        return rel.startswith(p) or any(
            fnmatch.fnmatchcase("/".join(rel.split("/")[:i]) + "/", p)
            for i in range(1, rel.count("/") + 1))
    if "/" not in p:
        return any(fnmatch.fnmatchcase(part, p) for part in rel.split("/"))
    return fnmatch.fnmatchcase(rel, p) or rel.startswith(p + "/")


def would_ship(rel: str, package: dict) -> bool:
    include = package.get("include")
    if include:
        return any(_pattern_hits(p, rel) for p in include)
    return not any(_pattern_hits(p, rel) for p in package.get("exclude", []))


def _dependency_tables(manifest: dict):
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        yield section, manifest.get(section, {})
    for target, table in manifest.get("target", {}).items():
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            yield f"target.{target}.{section}", table.get(section, {})


def inherited(root_manifest: dict, manifest: dict) -> dict:
    """Every workspace value the crate takes over, as cargo writes it out.

    `version.workspace = true` or `serde = { workspace = true }` puts the
    root's value into the published Cargo.toml. A change there changes the
    published crate without touching one of its files, so it is part of the
    content. Unknown keys are recorded as None, so a key that appears later in
    the root also reads as a change.
    """
    ws = root_manifest.get("workspace", {})
    ws_package = ws.get("package", {})
    ws_deps = ws.get("dependencies", {})
    out: dict = {"package": {}, "dependencies": {}, "lints": None}
    for key, value in manifest.get("package", {}).items():
        if isinstance(value, dict) and value.get("workspace") is True:
            out["package"][key] = ws_package.get(key)
    for section, table in _dependency_tables(manifest):
        for key, value in table.items():
            if isinstance(value, dict) and value.get("workspace") is True:
                out["dependencies"][f"{section}.{key}"] = ws_deps.get(key)
    lints = manifest.get("lints")
    if isinstance(lints, dict) and lints.get("workspace") is True:
        out["lints"] = ws.get("lints")
    return out


def local_version(root_manifest: dict, manifest: dict) -> str | None:
    version = manifest.get("package", {}).get("version")
    if isinstance(version, dict) and version.get("workspace") is True:
        return root_manifest.get("workspace", {}).get("package", {}).get("version")
    return version if isinstance(version, str) else None


def record_entry(repo: pathlib.Path, name: str, version: str,
                 package_dir: pathlib.Path, tarball: bytes) -> dict:
    """What the record keeps for one published-and-identical crate."""
    rel_dir = package_dir.resolve().relative_to(repo.resolve()).as_posix()
    published = tarball_files(tarball, name, version)
    files = {rel: _sha256(body) for rel, body in sorted(published.items())
             if rel not in GENERATED}
    tracked = git_tracked(repo, rel_dir)
    nested = nested_package_dirs(tracked)
    unshipped = sorted(
        rel for rel in tracked
        if rel not in published and rel != "Cargo.toml" and rel not in GENERATED
        and not _under(rel, nested))
    manifest = _toml(package_dir / "Cargo.toml")
    return {"name": name, "version": version, "dir": rel_dir, "files": files,
            "unshipped": unshipped,
            "inherited": inherited(_toml(repo / "Cargo.toml"), manifest)}


def load_record(path: pathlib.Path) -> list[dict]:
    try:
        data = json.loads(path.read_text())
    except (OSError, ValueError) as exc:
        raise Unreadable(f"{path}: {exc}") from exc
    if data.get("schema") != RECORD_SCHEMA or not isinstance(data.get("crates"), list):
        raise Unreadable(f"{path}: not a schema-{RECORD_SCHEMA} record")
    return data["crates"]


def write_record(path: pathlib.Path, entries: list[dict]) -> None:
    body = {
        "schema": RECORD_SCHEMA,
        "about": ("Written by scripts/release/published_version_matches_the_"
                  "workspace.py --all --record from crates.io; read offline by "
                  "--against-record in CI. Do not edit by hand (#358)."),
        "crates": sorted(entries, key=lambda e: (e["name"], e["version"])),
    }
    path.write_text(json.dumps(body, indent=1, sort_keys=True) + "\n")


def against_record(repo: pathlib.Path, entry: dict) -> tuple[str, list[str]]:
    """('moved' | 'same' | 'drifted', what differs) for one recorded crate."""
    package_dir = repo / entry["dir"]
    manifest = _toml(package_dir / "Cargo.toml")
    root = _toml(repo / "Cargo.toml")
    name = manifest.get("package", {}).get("name")
    if name != entry["name"]:
        raise Unreadable(f"the record puts {entry['name']} at {entry['dir']}, "
                         f"and the manifest there names {name!r}")
    version = local_version(root, manifest)
    if version != entry["version"]:
        return "moved", [f"now {version}, recorded {entry['version']}"]

    found: list[str] = []
    for rel, digest in sorted(entry["files"].items()):
        local = package_dir / ("Cargo.toml" if rel == ORIGINAL_MANIFEST else rel)
        if not local.is_file():
            found.append(f"{rel}: published, and not in the workspace")
        elif _sha256(local.read_bytes()) != digest:
            found.append(f"{rel}: published and local differ")

    tracked = git_tracked(repo, entry["dir"])
    nested = nested_package_dirs(tracked)
    known = set(entry["files"]) | set(entry["unshipped"]) | GENERATED
    package = manifest.get("package", {})
    for rel in sorted(tracked - known):
        if _under(rel, nested) or not would_ship(rel, package):
            continue
        found.append(f"{rel}: in the workspace, and not in the published crate")

    now = inherited(root, manifest)
    for part in ("package", "dependencies"):
        for key in sorted(set(now[part]) | set(entry["inherited"][part])):
            if now[part].get(key) != entry["inherited"][part].get(key):
                found.append(f"Cargo.toml: inherits another workspace {part} "
                             f"value for {key}")
    if now["lints"] != entry["inherited"]["lints"]:
        found.append("Cargo.toml: inherits other workspace lints")
    return ("drifted" if found else "same"), found


def report_drift(drifted: list[tuple[str, str, list[str]]]) -> None:
    print("", file=sys.stderr)
    print("A published version can never be replaced. Bump these and move "
          "every `=` pin that names them:", file=sys.stderr)
    for name, version, _ in drifted:
        print(f"  {name} {version}", file=sys.stderr)


def print_found(found: list[str]) -> None:
    for line in found[:5]:
        print(f"        {line}")
    if len(found) > 5:
        print(f"        … and {len(found) - 5} more")


def run_against_record(repo: pathlib.Path, record: pathlib.Path) -> int:
    try:
        entries = load_record(record)
    except Unreadable as exc:
        print(f"could not read the record, so this is not a verdict: {exc}",
              file=sys.stderr)
        return 2
    if not entries:
        print(f"SKIPPED (not a pass): {record} records no published crate",
              file=sys.stderr)
        return 2
    drifted: list[tuple[str, str, list[str]]] = []
    unreadable: list[str] = []
    for entry in entries:
        name, version = entry["name"], entry["version"]
        try:
            verdict, found = against_record(repo, entry)
        except Unreadable as exc:
            unreadable.append(f"{name}@{version}: {exc}")
            print(f"  ??  {name} {version} — could not compare")
            continue
        if verdict == "moved":
            print(f"  ok  {name} {version} — the workspace has moved on ({found[0]})")
        elif verdict == "same":
            print(f"  ok  {name} {version} — published, and identical to the record")
        else:
            drifted.append((name, version, found))
            print(f"  NO  {name} {version} — published with different content "
                  f"({len(found)} file(s))")
            print_found(found)
    if drifted:
        report_drift(drifted)
        return 1
    if unreadable:
        print("\nThe tree could not be compared, so this is not a verdict:",
              file=sys.stderr)
        for line in unreadable:
            print(f"  {line}", file=sys.stderr)
        return 2
    print(f"OK: {len(entries)} recorded crate(s), none changed under its "
          "published version")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--all", action="store_true",
                    help="every publishable crate in the workspace")
    ap.add_argument("--name")
    ap.add_argument("--version")
    ap.add_argument("--dir")
    ap.add_argument("--registry-base",
                    default=os.environ.get(BASE_ENV, DEFAULT_BASE))
    ap.add_argument("--record", action="store_true",
                    help="write the record of every published-and-identical crate")
    ap.add_argument("--against-record", action="store_true",
                    help="compare the tree with the record, offline")
    ap.add_argument("--record-file",
                    help=f"the record (default: <repo>/{RECORD_NAME.as_posix()})")
    ap.add_argument("--repo", help="the repository root (default: this checkout)")
    args = ap.parse_args()
    args.registry_base = args.registry_base.rstrip("/")

    repo = (pathlib.Path(args.repo).resolve() if args.repo
            else pathlib.Path(__file__).resolve().parents[2])
    record = pathlib.Path(args.record_file) if args.record_file else repo / RECORD_NAME

    if args.against_record:
        return run_against_record(repo, record)

    targets: list[tuple[str, str, pathlib.Path | None]]
    if args.all:
        targets = [(n, v, pathlib.Path(m).parent if m else None)
                   for n, v, m in workspace_crates(repo)]
    elif args.name and args.version and args.dir:
        targets = [(args.name, args.version, pathlib.Path(args.dir))]
    else:
        ap.error("either --all, --against-record, or --name with --version and --dir")
        return 2

    recorded: set[tuple[str, str]] | None = None
    if args.all and not args.record:
        try:
            recorded = {(e["name"], e["version"]) for e in load_record(record)}
        except Unreadable as exc:
            print(f"could not read the record, so this is not a verdict: {exc}",
                  file=sys.stderr)
            return 2

    drifted: list[tuple[str, str, list[str]]] = []
    unreadable: list[str] = []
    unrecorded: list[str] = []
    entries: list[dict] = []
    for name, version, package_dir in targets:
        try:
            verdict, found, tarball = check(args.registry_base, name, version,
                                            package_dir,
                                            repo if args.all else None)
            if verdict == "same" and args.record and tarball is not None \
                    and package_dir is not None:
                entries.append(record_entry(repo, name, version, package_dir,
                                            tarball))
        except Unreadable as exc:
            unreadable.append(f"{name}@{version}: {exc}")
            print(f"  ??  {name} {version} — could not read the registry")
            continue
        if verdict == "unpublished":
            print(f"  ok  {name} {version} — not on the registry yet")
        elif verdict == "same":
            print(f"  ok  {name} {version} — published, and identical")
            if recorded is not None and (name, version) not in recorded:
                unrecorded.append(f"{name} {version}")
        else:
            drifted.append((name, version, found))
            print(f"  NO  {name} {version} — published with different content "
                  f"({len(found)} file(s))")
            print_found(found)

    if drifted:
        report_drift(drifted)
        if args.record:
            print("Nothing was recorded: the record holds only versions that "
                  "match the registry.", file=sys.stderr)
        return 1
    if unreadable:
        print("", file=sys.stderr)
        print("The registry could not be read, so this is not a verdict:",
              file=sys.stderr)
        for line in unreadable:
            print(f"  {line}", file=sys.stderr)
        return 2
    if unrecorded:
        # The record is what CI compares with between releases. A published
        # version it does not hold is a crate CI no longer watches.
        print("", file=sys.stderr)
        print(f"{record} does not hold these published versions, so CI would "
              "not notice their content moving. Run with --all --record and "
              "commit the record:", file=sys.stderr)
        for line in unrecorded:
            print(f"  {line}", file=sys.stderr)
        return 1
    if args.record:
        write_record(record, entries)
        print(f"recorded {len(entries)} published crate(s) in {record}")
    print(f"OK: {len(targets)} crate(s), none published with other content")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
