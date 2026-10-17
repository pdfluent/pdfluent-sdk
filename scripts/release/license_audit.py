#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""license_audit.py — the licence field AND the licence text, on the artefact.

Every channel states the dual licence `AGPL-3.0-only OR
LicenseRef-PDFluent-Commercial` twice: once as metadata (the licence field)
and once as files (the licence text). Either half can go missing on its own —
a `files` array that forgets LICENSE-OFFER, a manifest whose expression a
validator cannot name — so this script checks both, on the built artefact
rather than on the source:

  every archive (.whl, .zip/.jar, .nupkg, .tgz/.tar.gz, sdist, unpacked dir):
      LICENSE* entries carrying both halves of the dual text, not an empty
      placeholder;
  wheels: the dist-info METADATA names the dual licence in License-Expression;
  sdists: the top-level PKG-INFO names it in License-Expression;
  npm tarballs and the wasm package (tarball or pkg dir): package.json names
      `SEE LICENSE IN LICENSE-OFFER` — npm rejects a LicenseRef expression,
      so the pointer is the channel's route (see docs/LICENSE_POLICY.toml
      [channel_representation]) — and the file it points at states the dual
      offer;
  --pom: the Maven <licenses> block names the dual licence exactly;
  --csproj: the NuGet project declares its licence file (PackageLicenseFile;
      NU5033 makes an expression and a file mutually exclusive, and nuget.org
      refuses the compound expression, so the file form is the channel's
      route — see docs/LICENSE_POLICY.toml [channel_representation]).

`twine check` and `npm pack --dry-run` stay manual commands in the runbook;
this script is what fails the lane.

Usage:
  python3 scripts/release/license_audit.py dist/*.whl dist/*.tar.gz [--pom bindings/java/pom.xml] [--csproj bindings/dotnet/src/PDFluent/PDFluent.csproj]

Tests: scripts/release/test_license_audit.py.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tarfile
import zipfile
from pathlib import Path

DUAL = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

# What an npm manifest publishes: npm's own documented route where no SPDX
# expression can spell the offer. Measured with validate-npm-package-license
# (see docs/LICENSE_POLICY.toml): the compound expression is invalid there.
NPM_LICENSE = "SEE LICENSE IN LICENSE-OFFER"

# The text a licence file must carry a trace of: the AGPL's own name, or the
# commercial offer. An empty LICENSE that satisfies a filename check satisfies
# nothing.
TEXT_MARKERS = ("AFFERO", "PDFluent")

# The two halves of the dual licence, as text markers. The AGPL file carries
# "AFFERO" and never "PDFluent"; the commercial file carries
# "PDFluent Commercial" and never "AFFERO" — each marker names its own half,
# so an archive holding only one half fails whichever half it lacks.
AGPL_MARKER = "AFFERO"
COMMERCIAL_MARKER = "PDFluent Commercial"


def _archive_entries(path: Path) -> dict[str, bytes]:
    """Every file in a zip- or tar-based artefact, name -> content."""
    name = path.name
    if name.endswith((".zip", ".whl", ".jar", ".nupkg")):
        with zipfile.ZipFile(path) as zf:
            return {i.filename: zf.read(i.filename) for i in zf.infolist() if not i.is_dir()}
    if name.endswith((".tar.gz", ".tgz")) or ".tar" in name:
        with tarfile.open(path, "r:*") as tf:
            out = {}
            for m in tf.getmembers():
                if m.isfile():
                    f = tf.extractfile(m)
                    out[m.name] = f.read() if f else b""
            return out
    raise SystemExit(f"unsupported artefact type: {path}")


def _dir_entries(path: Path) -> dict[str, bytes]:
    """Every file under an unpacked artefact dir (the wasm pkg dir), name -> content."""
    return {
        str(p.relative_to(path)): p.read_bytes()
        for p in sorted(path.rglob("*"))
        if p.is_file()
    }


def _license_expression(text: str) -> str | None:
    """The value of the License-Expression header, or None when absent."""
    for line in text.splitlines():
        name, sep, value = line.partition(":")
        if sep and name.strip().lower() == "license-expression":
            return value.strip()
    return None


def check_metadata_expression(kind: str, where: str, text: str) -> list[str]:
    """A wheel METADATA or sdist PKG-INFO names the dual licence exactly."""
    expr = _license_expression(text)
    if expr is None:
        return [f"{where}: {kind} carries no License-Expression (want {DUAL!r})"]
    if expr != DUAL:
        return [f"{where}: {kind} says {expr!r}, want {DUAL!r}"]
    return []


def check_npm_manifest(where: str, raw: bytes, entries: dict[str, bytes]) -> list[str]:
    """A packed package.json names the npm route, pointing at the dual offer.

    npm rejects a LicenseRef expression, so the manifest carries SEE LICENSE
    IN LICENSE-OFFER — a pointer, and a pointer dangles without its file.
    The file it names must be in the same artefact, stating the dual offer.
    """
    try:
        manifest = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        return [f"{where}: package.json is not valid JSON: {e}"]
    if not isinstance(manifest, dict):
        return [f"{where}: package.json is not an object"]
    lic = manifest.get("license")
    if lic != NPM_LICENSE:
        return [
            f"{where}: package.json license is {lic!r}, want {NPM_LICENSE!r} "
            "(npm rejects a LicenseRef expression)"
        ]
    pointed = NPM_LICENSE.removeprefix("SEE LICENSE IN ").strip()
    hits = [n for n in entries if os.path.basename(n) == pointed]
    if not hits:
        return [f"{where}: package.json points at {pointed}, which is not in the artefact"]
    for hit in sorted(hits):
        if DUAL in entries[hit].decode("utf-8", "replace"):
            return []
    return [
        f"{where}: {hits[0]} does not state the dual offer ({DUAL!r})"
    ]


def check_nested_npm_manifest(where: str, raw: bytes) -> list[str]:
    """A nested package.json is a module-type marker — unless it names one.

    Files like `dist/cjs/package.json` carry only `{"type": "commonjs"}`;
    without a `name` field there is no package here to judge, so they are
    skipped. A nested manifest WITH a name is a nested package, and it
    travels under the same licence route as the top one.
    """
    try:
        manifest = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        return [f"{where}: nested package.json is not valid JSON: {e}"]
    if not isinstance(manifest, dict):
        return [f"{where}: nested package.json is not an object"]
    if "name" not in manifest:
        return []
    lic = manifest.get("license")
    if lic != NPM_LICENSE:
        return [
            f"{where}: nested package {manifest.get('name')!r} names no "
            f"licence route (license is {lic!r}, want {NPM_LICENSE!r})"
        ]
    return []


def check_licence_texts(entries: dict[str, bytes], prefix: str = "") -> list[str]:
    """Both halves of the dual licence must be present as text.

    `entries` maps licence-file name to content (the artefact gate hands its
    CLI archive members here). An archive that holds only LICENSE-COMMERCIAL
    — or only the AGPL — states half the offer and fails naming the half.
    """
    texts = [c.decode("utf-8", "replace") for c in entries.values()]
    failures = []
    if not any(AGPL_MARKER in t for t in texts):
        failures.append(
            f"{prefix}no file carries the AGPL text "
            f"(need LICENSE with {AGPL_MARKER!r})"
        )
    if not any(COMMERCIAL_MARKER in t for t in texts):
        failures.append(
            f"{prefix}no file carries the commercial text "
            f"(need LICENSE-COMMERCIAL with {COMMERCIAL_MARKER!r})"
        )
    return failures


def _is_distribution_licence(name: str) -> bool:
    """Licence files the distribution itself offers, not vendored ones.

    An sdist vendors its path-dependency sources, and those carry their
    own texts (MIT/Apache for third-party crates, font licences under
    assets/): judging every LICENSE* in the tree fails a correct sdist
    on bytes that are supposed to say something else. Only the top level
    of the distribution is judged -- the files the distribution offers
    its licence under -- plus a wheel's dist-info/licenses/ dir, which
    maturin fills exactly from the project's license-files list.
    """
    parts = name.split("/")
    if len(parts) <= 2:
        return True
    return parts[-2] == "licenses"


def check_archive(path: Path) -> list[str]:
    if path.is_dir():
        try:
            entries = _dir_entries(path)
        except OSError as e:
            return [f"{path}: unreadable artefact dir: {e}"]
    elif path.is_file():
        try:
            entries = _archive_entries(path)
        except (zipfile.BadZipFile, tarfile.TarError, OSError) as e:
            return [f"{path}: unreadable artefact: {e}"]
    else:
        return [f"{path}: file not found"]
    licensed = {
        n: c for n, c in entries.items()
        if os.path.basename(n).lower().startswith("license")
        and _is_distribution_licence(n)
    }
    if not licensed:
        return [f"{path}: no LICENSE file inside the artefact"]
    failures = []
    for lic_name, content in sorted(licensed.items()):
        text = content.decode("utf-8", "replace")
        if len(content) < 100 or not any(m in text for m in TEXT_MARKERS):
            failures.append(
                f"{path}: {lic_name} does not carry the licence text "
                f"({len(content)} bytes, no AGPL/PDFluent marker)"
            )
    failures.extend(check_licence_texts(licensed, prefix=f"{path}: "))
    failures.extend(_check_metadata_fields(path, entries))
    return failures


def _check_metadata_fields(path: Path, entries: dict[str, bytes]) -> list[str]:
    """The licence field beside the licence files, per artefact kind.

    A wheel without License-Expression in METADATA, an sdist without it in
    PKG-INFO, or an npm tarball whose package.json names another licence
    all publish under terms the files beside them contradict — so each
    kind's field is read here, on the artefact, not on the source.

    Only the top-level manifest is judged; nested package.json files
    without a `name` are module-type markers and are skipped.
    """
    is_wheel = path.suffix == ".whl" or any(
        ".dist-info/" in n for n in entries
    )
    if is_wheel:
        metas = [n for n in entries if n.endswith(".dist-info/METADATA")]
        if not metas:
            return [f"{path}: wheel carries no dist-info METADATA"]
        return check_metadata_expression(
            "wheel METADATA", f"{path}: {metas[0]}",
            entries[metas[0]].decode("utf-8", "replace"),
        )
    npm_manifests = [
        n for n in entries
        if os.path.basename(n) == "package.json"
        and ("/" not in n or n.startswith("package/"))
    ]
    if npm_manifests:
        # The licence is judged on the top-level manifest only. Generated
        # trees carry nested package.json files — `{"type": "commonjs"}`
        # markers under dist/ — that name no package and carry no licence;
        # judging the first manifest in tarball order failed a correct
        # tarball on its own marker file.
        top = next(
            (n for n in ("package/package.json", "package.json")
             if n in entries),
            None,
        )
        if top is None:
            return check_npm_manifest(
                f"{path}: {npm_manifests[0]}", entries[npm_manifests[0]],
                entries,
            )
        failures = check_npm_manifest(
            f"{path}: {top}", entries[top], entries
        )
        for nested in sorted(n for n in npm_manifests if n != top):
            failures.extend(
                check_nested_npm_manifest(f"{path}: {nested}", entries[nested])
            )
        return failures
    infos = [n for n in entries if os.path.basename(n) == "PKG-INFO"]
    if infos:
        return check_metadata_expression(
            "sdist PKG-INFO", f"{path}: {infos[0]}",
            entries[infos[0]].decode("utf-8", "replace"),
        )
    return []


def check_pom(pom: Path) -> list[str]:
    if not pom.is_file():
        return [f"{pom}: file not found"]
    text = pom.read_text(encoding="utf-8")
    want = f"<name>{DUAL}</name>"
    # Exact element, not a substring: "... OR ... OR MIT" must not pass.
    if not re.search(rf"^\s*{re.escape(want)}\s*$", text, re.M):
        return [f"{pom}: <licenses> must name exactly {DUAL!r}"]
    return []


def check_csproj(csproj: Path) -> list[str]:
    if not csproj.is_file():
        return [f"{csproj}: file not found"]
    text = csproj.read_text(encoding="utf-8")
    m = re.search(r"<PackageLicenseFile>([^<]+)</PackageLicenseFile>", text)
    if not m:
        return [f"{csproj}: no <PackageLicenseFile> declared"]
    lic = csproj.parent / m.group(1).strip()
    if not lic.is_file():
        return [f"{csproj}: declared licence file missing: {lic}"]
    content = lic.read_text(encoding="utf-8", errors="replace")
    if DUAL not in content and "AGPL" not in content:
        return [f"{lic}: licence file does not state the dual licence"]
    return []


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Licence audit over built artefacts.")
    ap.add_argument("artifacts", nargs="*", type=Path)
    ap.add_argument("--pom", type=Path, default=None)
    ap.add_argument("--csproj", type=Path, default=None)
    args = ap.parse_args(argv)
    if not args.artifacts and args.pom is None and args.csproj is None:
        ap.error("nothing to check: pass artefacts and/or --pom/--csproj")

    failures: list[str] = []
    for art in args.artifacts:
        failures.extend(check_archive(art))
    if args.pom is not None:
        failures.extend(check_pom(args.pom))
    if args.csproj is not None:
        failures.extend(check_csproj(args.csproj))

    if failures:
        print("LICENSE-AUDIT: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("LICENSE-AUDIT: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
