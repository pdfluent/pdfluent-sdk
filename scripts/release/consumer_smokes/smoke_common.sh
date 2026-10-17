#!/usr/bin/env bash
# smoke_common.sh — one argument contract for every consumer smoke.
#
# Sourced, never executed: each smoke_*.sh in this directory sources this
# file and binds the three contract flags to the shared state below, so the
# parsing has one implementation instead of eight copies that drift.
#
# Contract flags (every smoke accepts all three):
#   --artifact PATH  the file under test; per-smoke aliases keep working
#                    (--wheel, --jar, --nupkg, --pkg-dir, ...) but bind the
#                    same artefact
#   --platform KEY   must name this host (e.g. linux-arm64-musl on that
#                    host); the smoke runs where it is, against the given
#                    artefact, so a foreign platform is refused
#   --cross-check    run even when --platform names another host
#
# An unknown flag exits 2 naming itself: a typo'd flag silently ignored
# vouches for whatever the defaults pick. A --platform naming another host
# is refused (exit 2) unless --cross-check says the operator means it.
#
# Per-smoke use (after `set -Eeuo pipefail`, before the parse loop):
#   . "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/smoke_common.sh"
#   SMOKE_TAG="[smoke_name]"
# in the loop:
#   --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
#   --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
#   *) smoke_unknown_option "$1" ;;
# after the loop, before anything else is resolved:
#   smoke_check_platform
if [[ -z "${SMOKE_COMMON_SOURCED:-}" ]]; then
SMOKE_COMMON_SOURCED=1
SMOKE_PLATFORM="${SMOKE_PLATFORM:-}"
SMOKE_CROSS_CHECK="${SMOKE_CROSS_CHECK:-0}"
SMOKE_TAG="${SMOKE_TAG:-[smoke]}"
fi

smoke_unknown_option() { # $1 = the flag
    echo "$SMOKE_TAG unknown option: $1" >&2
    exit 2
}

# The host's own platform key, in the repo's key spelling (linux-x64,
# linux-arm64-musl, osx-arm64, win-x64, ...).
smoke_host_platform() {
    local os arch
    os="$(uname -s)"; arch="$(uname -m)"
    if [[ "$arch" == "arm64" ]]; then arch="aarch64"; fi
    case "$os" in
        Darwin)
            case "$arch" in
                x86_64) echo "osx-x64" ;;
                aarch64) echo "osx-arm64" ;;
                *) echo "osx-$arch" ;;
            esac ;;
        Linux)
            local libc=""
            # musl's `ldd --version` prints its banner and exits 1, so under
            # the callers' pipefail a `ldd --version | grep` pipeline fails even
            # when grep matches: capture first, then match. The loader file is
            # the second witness.
            local ldd_out=""
            if command -v ldd >/dev/null 2>&1; then ldd_out="$(ldd --version 2>&1 || true)"; fi
            if [[ "$ldd_out" == *[Mm]usl* ]] || compgen -G '/lib/ld-musl-*.so.1' >/dev/null; then
                libc="-musl"
            fi
            case "$arch" in
                x86_64) echo "linux-x64$libc" ;;
                aarch64) echo "linux-arm64$libc" ;;
                *) echo "linux-$arch$libc" ;;
            esac ;;
        MINGW*|MSYS*|CYGWIN*) echo "win-x64" ;;
        *) echo "$os-$arch" ;;
    esac
}

# Aliases operators actually type: macos-/darwin- for osx-, full arch names.
smoke_normalize_platform() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' \
        | sed -e 's/^macos-/osx-/' -e 's/^darwin-/osx-/' \
              -e 's/x86_64/x64/g' -e 's/aarch64/arm64/g'
}

# Refuse a --platform naming another host, unless --cross-check overrides.
# Runs before any artefact is resolved, so the refusal names the platform
# rather than whatever the artefact check would have said.
smoke_check_platform() {
    if [[ -n "$SMOKE_PLATFORM" && "$SMOKE_CROSS_CHECK" == "0" ]]; then
        local host_key
        host_key="$(smoke_host_platform)"
        if [[ "$(smoke_normalize_platform "$SMOKE_PLATFORM")" != "$(smoke_normalize_platform "$host_key")" ]]; then
            echo "$SMOKE_TAG platform mismatch: --platform $SMOKE_PLATFORM but this host is $host_key (pass --cross-check to run anyway)" >&2
            exit 2
        fi
    fi
}

# A path in the spelling a native Windows program reads. Git Bash (MSYS)
# rewrites a bare /c/... argument for a native program, but not a path written
# into a file (nuget.config, Cargo.toml, pom.xml) or glued to an option
# (-Dfile=, -I, /I): those reach the program as /c/... and miss (15-10 NU1301
# on win-x64). `smoke_native_path` gives C:\... ; `smoke_native_path -m` gives
# C:/... for files where a backslash is an escape (TOML, JSON). Off Windows
# (no cygpath) the path comes back unchanged.
smoke_native_path() {
    local mode="-w"
    if [[ "${1:-}" == "-m" ]]; then mode="-m"; shift; fi
    if command -v cygpath >/dev/null 2>&1; then cygpath "$mode" "$1"; else printf '%s\n' "$1"; fi
}

# True on Git Bash / MSYS2 / Cygwin, where a native program reads /c/x as
# <current drive>:\c\x.
smoke_is_msys() {
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) return 0 ;;
    esac
    return 1
}

# The RFC 8089 file:// URL (no trailing slash) for an absolute local path, in
# the spelling a native program on this host reads: file:///C:/x on Windows,
# file:///tmp/x elsewhere. Glueing "file://" to a Git Bash path gave
# file:///c/x, which the native win-x64 CLI opened as \c\x on the current
# drive (16-10, os error 3). cygpath -m when present, else the MSYS /c/x
# spelling rewritten by hand; a path already in C:/x or C:\x form keeps its
# drive. A relative path is refused: a URL has no working directory.
smoke_file_url() {
    local p="$1"
    if command -v cygpath >/dev/null 2>&1; then
        p="$(cygpath -m "$p")" || return 1
    elif [[ "$p" =~ ^/([A-Za-z])/(.*)$ ]] && smoke_is_msys; then
        p="$(printf '%s' "${BASH_REMATCH[1]}" | tr '[:lower:]' '[:upper:]'):/${BASH_REMATCH[2]}"
    fi
    p="${p//\\//}"
    if [[ "$p" =~ ^[A-Za-z]:/ ]]; then
        printf 'file:///%s\n' "${p%/}"
    elif [[ "$p" == /* ]]; then
        printf 'file://%s\n' "${p%/}"
    else
        echo "$SMOKE_TAG not an absolute path, no file URL: $1" >&2
        return 1
    fi
}
