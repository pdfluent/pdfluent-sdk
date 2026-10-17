#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The Maven channel guard reads the release, it does not assume one (#358).

The version half of `scripts/release/maven_channel_guard.sh` used to demand
`1.0.0-beta.N` in the canonical pom. That rule was written while the channel
was on release candidates and it went stale the day the release became 1.0.0:
on the publish morning it refused a pom that `every_registry_names_the_same
_release.py` had just approved, and section 3.6 of the registry runbook runs
this guard before `mvn deploy`. A rule that names one release cannot survive
the next one.

It was also looser than it read. `grep -qE '<version>1\\.0\\.0-beta\\.[0-9]+'`
matches ANY `<version>` element in the file, so a dependency or a plugin on a
beta number satisfied it while the project's own coordinates said something
else entirely -- case 5 below.

The repair is to compare, not to recognise: the pom's own version must be the
version `[workspace.package]` names, read through the same two functions the
six-manifest guard uses, so the two can never disagree about what a release is.

Cases 1 and 4 are red on the old rule; case 5 is red on it too, in the other
direction. Case 3 is the one the old rule was right about and is kept so the
repair cannot quietly drop it.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/release/maven_channel_guard.sh"
CANON = "bindings/java/pom.xml"
LEGACY = "crates/pdf-java/pom.xml"
WORKSPACE = "Cargo.toml"

MINIMUM_CASES = 9  # FLOOR


def fixture(root: pathlib.Path, *, release: str, pom: str, extra_pom: str = "") -> None:
    """A tree the guard can run in, with the two versions set independently."""
    for rel in (GUARD, CANON, LEGACY, WORKSPACE,
                "scripts/release/maven_canonical_version_pair.py",
                "scripts/release/artifacts_cover_every_platform.py",
                "scripts/release/license_audit.py",
                "scripts/ci/every_registry_names_the_same_release.py"):
        source = REPO / rel
        if not source.exists():          # the legacy pom may be gone for good
            continue
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    shutil.copymode(REPO / GUARD, root / GUARD)

    # The release the whole tree is on.
    text = (root / WORKSPACE).read_text(encoding="utf-8")
    head, rest = text.split("[workspace.package]", 1)
    rest = re.sub(r'^version\s*=\s*"[^"]+"', f'version = "{release}"', rest, count=1,
                  flags=re.M)
    (root / WORKSPACE).write_text(head + "[workspace.package]" + rest, encoding="utf-8")

    # The version the canonical pom names for itself, and anything extra.
    pom_text = (root / CANON).read_text(encoding="utf-8")
    before, after = pom_text.split("<dependencies>", 1)
    before = re.sub(r"<version>[^<]+</version>", f"<version>{pom}</version>", before,
                    count=1)
    (root / CANON).write_text(before + extra_pom + "<dependencies>" + after,
                              encoding="utf-8")


def run(root: pathlib.Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["bash", str(root / GUARD), *args], capture_output=True,
                          text=True, cwd=root)


def _native_bytes(plat: str) -> bytes:
    """1 MB the artefact gate reads as this platform's native.

    A valid header per slot (kind, arch, libc markers) padded past the
    per-native floor of 1 MB x 0.8: zeros would fail the identity check, and
    a short file would fail the floor. The ELF fixtures carry a minimal but
    valid dynamic section — one PT_LOAD plus one PT_DYNAMIC whose DT_NEEDED
    names the slot's libc — because the gate reads DT_NEEDED, not stray
    marker bytes: a header with the soname pasted behind it reads as "no
    libc marker" and fails the slot it was built to fill.
    """
    import struct

    if plat.startswith("win"):
        head = b"MZ" + b"\0" * 58 + struct.pack("<I", 0x40)
        return (head + b"PE\0\0" + struct.pack("<H", 0x8664)).ljust(
            1_000_000, b"\0")
    if plat.startswith("osx"):
        cputype = 0x0100000C if plat == "osx-arm64" else 0x01000007
        return (b"\xcf\xfa\xed\xfe" + struct.pack("<I", cputype)).ljust(
            1_000_000, b"\0")
    machine = 183 if "arm64" in plat else 62
    arch = "aarch64" if "arm64" in plat else "x86-64"
    soname = f"libc.musl-{arch}.so.1" if "musl" in plat else "libc.so.6"
    size = 1_000_000
    blob = bytearray(size)
    blob[0:4] = b"\x7fELF"
    blob[4] = 2  # 64-bit
    blob[5] = 1  # little-endian
    struct.pack_into("<HHI", blob, 16, 3, machine, 1)  # ET_DYN
    struct.pack_into("<Q", blob, 32, 64)  # e_phoff
    struct.pack_into("<H", blob, 54, 56)  # e_phentsize
    struct.pack_into("<H", blob, 56, 2)  # e_phnum
    struct.pack_into(
        "<IIQQQQQQ", blob, 64, 1, 5, 0, 0x400000, 0, size, size, 0x1000
    )  # PT_LOAD
    dyn_off, str_off = 512, 576
    struct.pack_into(
        "<IIQQQQQQ", blob, 120, 2, 4, dyn_off, 0x400000 + dyn_off, 0, 64, 64, 8
    )  # PT_DYNAMIC
    strtab = b"\0" + soname.encode() + b"\0"
    struct.pack_into("<qQ", blob, dyn_off, 1, 1)  # DT_NEEDED
    struct.pack_into("<qQ", blob, dyn_off + 16, 5, 0x400000 + str_off)  # DT_STRTAB
    struct.pack_into("<qQ", blob, dyn_off + 32, 10, len(strtab))  # DT_STRSZ
    struct.pack_into("<qQ", blob, dyn_off + 48, 0, 0)  # DT_NULL
    blob[str_off : str_off + len(strtab)] = strtab
    return bytes(blob)


def write_jar(path: pathlib.Path, *, drop_platform: str | None = None) -> None:
    """A fixture jar: both libs for all seven platforms, unless one is dropped.

    Natives are 1 MB each so the artefact gate's per-native floor (1 MB x 0.8)
    holds on the complete fixture, with both licence texts beside them.
    """
    import zipfile

    plats = ("linux-x64", "linux-arm64", "linux-x64-musl", "linux-arm64-musl",
             "osx-x64", "osx-arm64", "win-x64")
    libs = {"linux": ("libpdfluent_java.so", "libpdf_capi.so"),
            "osx": ("libpdfluent_java.dylib", "libpdf_capi.dylib"),
            "win": ("pdfluent_java.dll", "pdf_capi.dll")}
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_STORED) as zf:
        zf.writestr("com/pdfluent/Document.class", b"\xca\xfe\xba\xbe")
        zf.writestr("META-INF/LICENSE",
                    b"GNU AFFERO GENERAL PUBLIC LICENSE\n" + b"t\n" * 100)
        zf.writestr("META-INF/LICENSE-COMMERCIAL",
                    b"PDFluent Commercial Licence\n" + b"t\n" * 100)
        for plat in plats:
            if plat == drop_platform:
                continue
            os_part = "win" if plat.startswith("win") else (
                "osx" if plat.startswith("osx") else "linux")
            for lib in libs[os_part]:
                zf.writestr(f"native/{plat}/{lib}", _native_bytes(plat))


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:600]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as tmp:
        # 1. The release as it actually is on the publish day. Red on the old rule.
        root = pathlib.Path(tmp) / "ga"
        fixture(root, release="1.0.0", pom="1.0.0")
        done = run(root)
        case("a 1.0.0 pom on a 1.0.0 workspace passes",
             done.returncode == 0, done.stdout + done.stderr)

        # 2. The pom naming a release the workspace does not.
        root = pathlib.Path(tmp) / "drift"
        fixture(root, release="1.0.0", pom="0.9.0")
        done = run(root)
        case("a pom that names another version is refused",
             done.returncode == 1 and "0.9.0" in done.stdout, done.stdout + done.stderr)

        # 3. The case the old rule existed for: a release candidate still agrees.
        root = pathlib.Path(tmp) / "beta"
        fixture(root, release="1.0.0-beta.19", pom="1.0.0-beta.19")
        done = run(root)
        case("a beta release still passes when both name it",
             done.returncode == 0, done.stdout + done.stderr)

        # 4. The drift the old rule could not see: it matched the shape, not the release.
        root = pathlib.Path(tmp) / "stale-beta"
        fixture(root, release="1.0.0", pom="1.0.0-beta.19")
        done = run(root)
        case("a pom left on the last beta is refused on a 1.0.0 workspace",
             done.returncode == 1 and "1.0.0-beta.19" in done.stdout,
             done.stdout + done.stderr)

        # 5. A version elsewhere in the file is not the project's version.
        root = pathlib.Path(tmp) / "laundered"
        fixture(root, release="1.0.0", pom="0.9.0",
                extra_pom="<build><plugins><plugin>"
                          "<version>1.0.0-beta.19</version>"
                          "</plugin></plugins></build>\n")
        done = run(root)
        case("a matching version elsewhere in the pom does not launder the project's",
             done.returncode == 1, done.stdout + done.stderr)

        # 6. The guard is still the guard: identity is checked alongside the version.
        root = pathlib.Path(tmp) / "identity"
        fixture(root, release="1.0.0", pom="1.0.0")
        canon = root / CANON
        canon.write_text(canon.read_text(encoding="utf-8")
                         .replace("<artifactId>pdfluent</artifactId>",
                                  "<artifactId>xfa-pdf</artifactId>", 1),
                         encoding="utf-8")
        done = run(root)
        case("a renamed artefact is still refused",
             done.returncode == 1 and "artifactId" in done.stdout,
             done.stdout + done.stderr)

        # 7. No jar built: a warning in the dev lane, a failure with --require-jar.
        root = pathlib.Path(tmp) / "no-jar"
        fixture(root, release="1.0.0", pom="1.0.0")
        done = run(root)
        case("no built jar warns but passes in the dev lane",
             done.returncode == 0 and "contents unchecked" in done.stdout,
             done.stdout + done.stderr)
        done = run(root, "--require-jar")
        case("no built jar fails the release lane",
             done.returncode == 1 and "no built jar" in done.stdout,
             done.stdout + done.stderr)

        # 8. The 1.0.0 defect: a jar whose /native/ misses a platform is refused,
        # with the platform named; a complete jar passes.
        root = pathlib.Path(tmp) / "thin-jar"
        fixture(root, release="1.0.0", pom="1.0.0")
        write_jar(root / "bindings/java/target/pdfluent-1.0.0.jar",
                  drop_platform="linux-arm64-musl")
        done = run(root, "--require-jar")
        case("a jar missing a platform is refused with its name",
             done.returncode == 1 and "linux-arm64-musl" in done.stdout,
             done.stdout + done.stderr)
        root = pathlib.Path(tmp) / "full-jar"
        fixture(root, release="1.0.0", pom="1.0.0")
        write_jar(root / "bindings/java/target/pdfluent-1.0.0.jar")
        done = run(root, "--require-jar")
        case("a jar covering every platform passes",
             done.returncode == 0, done.stdout + done.stderr)

        # 9. requireFilesExist passes a 0-byte file, so the pom also sizes
        # every staged native. Read off the real pom: all fourteen files
        # under a requireFilesSize with a nonzero minsize -- and an explicit
        # huge maxsize, because the rule's own default max is 10000 bytes:
        # build 6 set minsize only and the enforcer refused every real
        # native as "too large. Max. is 10000".
        pom_text = (REPO / CANON).read_text(encoding="utf-8")
        size_block = pom_text.split("<requireFilesSize>", 1)
        size_body = size_block[1].split("</requireFilesSize>", 1)[0] \
            if len(size_block) == 2 else ""
        sized_ok = (
            bool(size_body)
            and "<minsize>100000</minsize>" in size_body
            and "<maxsize>9223372036854775807</maxsize>" in size_body
            and all(
                f"native/{plat}/" in size_body
                for plat in ("linux-x64", "linux-arm64", "linux-x64-musl",
                             "linux-arm64-musl", "osx-x64", "osx-arm64",
                             "win-x64")
            )
        )
        case("the pom sizes every staged native, with no effective maximum",
             sized_ok, pom_text[:600])

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: the Maven channel guard compares the pom with the release")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
