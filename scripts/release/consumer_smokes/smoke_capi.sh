#!/usr/bin/env bash
# smoke_capi.sh — Consumer smoke test for the PDFluent C API archive.
#
# Unpacks the release archive (include/ + lib/ layout from package_cabi.sh)
# into a clean directory, compiles a small C program against the unpacked
# headers, and extracts a fixture PDF to markdown through the C ABI: open,
# extract_markdown, free. The markdown must contain a known sentence.
#
# The smoke program loads the shared library at RUNTIME (dlopen /
# LoadLibrary) rather than linking it. That is deliberate: the Windows
# archive ships the .dll with no import library, so a link-time smoke would
# need a per-platform import-lib dance; the runtime load proves the cdylib's
# exported symbols resolve on the host, which is what a link would prove
# too, on every platform with one code path.
#
# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_capi.sh [--artifact PATH]
#       [--version VER] [--platform KEY] [--cross-check]
#       [--fixture PATH] [--sentence TEXT]
#
# Options:
#   --artifact PATH  Release archive (.tar.gz/.zip) to unpack (dev lane
#                    default: newest dist/capi/pdfluent-c-*.tar.gz)
#   --version VER    Expected version string (default: any)
#   --platform KEY   Must name this host; refused otherwise unless --cross-check
#   --cross-check    Run even when --platform names another host
#   --fixture PATH   Fixture PDF to extract (default: fixtures/structured.pdf)
#   --sentence TEXT  Known sentence the markdown must contain (default: Safety rules)
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.
#
# Lanes: in a release lane (PDFLUENT_RELEASE_LANE=1) --artifact is required
# and a missing C compiler is a failure, not a skip.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_capi]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
ARTIFACT=""
ARTIFACT_GIVEN=0
EXPECTED_VERSION=""
FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"
SENTENCE="Safety rules"
STRICT="${PDFLUENT_RELEASE_LANE:-0}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact) ARTIFACT="$2"; ARTIFACT_GIVEN=1; shift 2 ;;
    --version) EXPECTED_VERSION="$2"; shift 2 ;;
    --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
    --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
    --fixture) FIXTURE="$2"; shift 2 ;;
    --sentence) SENTENCE="$2"; shift 2 ;;
    -h|--help) sed -n '/^# Usage:/,/^#   --sentence/p' "$0"; exit 0 ;;
    *) smoke_unknown_option "$1" ;;
  esac
done

smoke_check_platform

echo "=== PDFluent C API consumer smoke test ==="
[[ -n "$SMOKE_PLATFORM" ]] && echo "Platform: $SMOKE_PLATFORM"

skip_or_fail() { # $1 = reason
  if [[ "$STRICT" == "1" ]]; then
    echo "[smoke_capi] FAIL (release lane: a skip is a failure): $1"
    exit 1
  fi
  echo "SKIPPED (not a pass): $1"
  exit 0
}

# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
unset PDFLUENT_LICENSE_KEY

[[ -f "$FIXTURE" ]] || { echo "[smoke_capi] FAIL: fixture not found: $FIXTURE"; exit 1; }

# A given artefact must exist, naming the path: a typo'd --artifact that
# skipped instead would vouch for nothing while reading green.
if [[ "$ARTIFACT_GIVEN" == "1" ]]; then
  [[ -f "$ARTIFACT" ]] || { echo "[smoke_capi] no such artifact: $ARTIFACT" >&2; exit 2; }
elif [[ "$STRICT" == "1" ]]; then
  echo "[smoke_capi] release lane needs --artifact: auto-detect could vouch for any archive in the tree" >&2
  exit 2
else
  ARTIFACT="$(ls -t "$REPO_ROOT"/dist/capi/pdfluent-c-*.tar.gz 2>/dev/null | head -1)"
fi
[[ -n "$ARTIFACT" && -f "$ARTIFACT" ]] || skip_or_fail "no C API archive (looked under dist/capi/pdfluent-c-*.tar.gz; pass --artifact)"
echo "Artifact: $ARTIFACT"

# The first compiler on PATH wins. Plain `cc` covers gcc/clang everywhere
# including the musl container and macOS; MSVC's `cl` (no cc on a bare
# Windows runner) compiles the same file — the program needs no link flags,
# so no per-compiler switch is required beyond the output name.
CC_BIN=""
for candidate in cc gcc clang cl; do
  if command -v "$candidate" >/dev/null 2>&1; then CC_BIN="$candidate"; break; fi
done
[[ -n "$CC_BIN" ]] || skip_or_fail "no C compiler on PATH (looked for cc, gcc, clang, cl)"
echo "Compiler: $CC_BIN"

SMOKE_DIR="$(mktemp -d -t pdfluent_capi_smoke_XXXXXX)"
trap 'rm -rf "$SMOKE_DIR"' EXIT

case "$ARTIFACT" in
  *.tar.gz|*.tgz) tar -xzf "$ARTIFACT" -C "$SMOKE_DIR" ;;
  *.zip) python3 -c "import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$ARTIFACT" "$SMOKE_DIR" ;;
  *) echo "[smoke_capi] FAIL: unknown archive type: $ARTIFACT"; exit 1 ;;
esac

# The archive root is the package dir (include/ + lib/ directly beneath it).
PKG_DIR="$(find "$SMOKE_DIR" -maxdepth 2 -type d -name include | head -1)"
[[ -n "$PKG_DIR" ]] || { echo "[smoke_capi] FAIL: no include/ dir in $ARTIFACT"; exit 1; }
PKG_DIR="$(dirname "$PKG_DIR")"
[[ -f "$PKG_DIR/include/pdfluent.h" ]] || { echo "[smoke_capi] FAIL: no include/pdfluent.h in $ARTIFACT"; exit 1; }
LIB_FILE="$(find "$PKG_DIR/lib" -type f \( -name '*.so' -o -name '*.dylib' -o -name '*.dll' \) | head -1)"
[[ -n "$LIB_FILE" && -f "$LIB_FILE" ]] || { echo "[smoke_capi] FAIL: no shared library under lib/ in $ARTIFACT"; exit 1; }
echo "Headers: $PKG_DIR/include/pdfluent.h"
echo "Library: $LIB_FILE"

if [[ -n "$EXPECTED_VERSION" ]]; then
  basename "$ARTIFACT" | grep -q "$EXPECTED_VERSION" || {
    echo "[smoke_capi] FAIL: artifact $(basename "$ARTIFACT") does not match --version $EXPECTED_VERSION"
    exit 1
  }
fi

# The consumer program. It includes the shipped header — the compile proves
# the header stands alone — and calls the ABI through four function pointers
# resolved at runtime (dlopen / LoadLibrary), so no import library or link
# flag is needed on any platform.
cat > "$SMOKE_DIR/smoke.c" <<'C_EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "pdfluent.h"
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

typedef PdfStatus (*open_fn)(const char *, const char *, PdfDocument **);
typedef char *(*markdown_fn)(const PdfDocument *, int32_t, int32_t);
typedef void (*free_doc_fn)(PdfDocument *);
typedef void (*free_str_fn)(char *);

static void *lib_handle = NULL;

static void *load_symbol(const char *lib, const char *name) {
#ifdef _WIN32
  if (!lib_handle) {
    lib_handle = (void *)LoadLibraryA(lib);
    if (!lib_handle) {
      fprintf(stderr, "LoadLibrary failed for %s\n", lib);
      exit(1);
    }
  }
  void *sym = (void *)GetProcAddress((HMODULE)lib_handle, name);
#else
  if (!lib_handle) {
    lib_handle = dlopen(lib, RTLD_NOW);
    if (!lib_handle) {
      fprintf(stderr, "dlopen failed for %s: %s\n", lib, dlerror());
      exit(1);
    }
  }
  dlerror();
  void *sym = dlsym(lib_handle, name);
#endif
  if (!sym) {
    fprintf(stderr, "symbol not found: %s\n", name);
    exit(1);
  }
  return sym;
}

int main(int argc, char **argv) {
  if (argc != 4) {
    fprintf(stderr, "usage: smoke <lib> <fixture> <sentence>\n");
    return 2;
  }
  const char *lib = argv[1];
  const char *fixture = argv[2];
  const char *sentence = argv[3];

  open_fn pdf_document_open = (open_fn)load_symbol(lib, "pdf_document_open");
  markdown_fn pdf_document_extract_markdown =
      (markdown_fn)load_symbol(lib, "pdf_document_extract_markdown");
  free_doc_fn pdf_document_free = (free_doc_fn)load_symbol(lib, "pdf_document_free");
  free_str_fn pdf_string_free = (free_str_fn)load_symbol(lib, "pdf_string_free");

  PdfDocument *doc = NULL;
  PdfStatus st = pdf_document_open(fixture, NULL, &doc);
  if (st != PDF_STATUS_OK || !doc) {
    fprintf(stderr, "FAIL: pdf_document_open status %d\n", (int)st);
    return 1;
  }
  char *markdown = pdf_document_extract_markdown(doc, 0, -1);
  if (!markdown) {
    fprintf(stderr, "FAIL: pdf_document_extract_markdown returned NULL\n");
    pdf_document_free(doc);
    return 1;
  }
  int ok = strstr(markdown, sentence) != NULL;
  if (!ok) {
    fprintf(stderr, "FAIL: '%s' not in markdown output:\n", sentence);
    fprintf(stderr, "%.500s\n", markdown);
    pdf_string_free(markdown);
    pdf_document_free(doc);
    return 1;
  }
  printf("  extract_markdown: OK (%lu chars, '%s' found)\n",
         (unsigned long)strlen(markdown), sentence);
  char *nl = strchr(markdown, '\n');
  if (nl) *nl = '\0';
  printf("  first line: %s\n", markdown);
  pdf_string_free(markdown);
  pdf_document_free(doc);
  printf("C API consumer smoke PASS\n");
  return 0;
}
C_EOF

echo "[smoke_capi] compiling ..."
if [[ "$CC_BIN" == "cl" ]]; then
  # cl takes -I/-Fe as well as /I//Fe: with a leading slash Git Bash takes
  # the whole option for a POSIX path and rewrites it; the paths themselves
  # in native spelling, since an option-glued path is never converted.
  ( cd "$SMOKE_DIR" && cl -nologo "-I$(smoke_native_path "${PKG_DIR}/include")" smoke.c "-Fe$(smoke_native_path "${SMOKE_DIR}/smoke.exe")" > compile.log 2>&1 ) || {
    echo "[smoke_capi] FAIL: compile failed:"; sed 's/^/    /' "$SMOKE_DIR/compile.log"; exit 1
  }
  SMOKE_BIN="$SMOKE_DIR/smoke.exe"
else
  # glibc before 2.34 keeps dlopen out of libc; musl and macOS need no flag.
  DL_FLAG=""
  [[ "$(uname -s)" == "Linux" ]] && DL_FLAG="-ldl"
  # macOS cc targets the hardware, not the shell: under Rosetta it still
  # built an arm64 program that could not load the osx-x64 dylib (macOS
  # smoke run 37355713891). Name the platform's architecture outright.
  ARCH_FLAG=""
  if [[ "$(uname -s)" == "Darwin" ]]; then
    case "$SMOKE_PLATFORM" in
      osx-x64) ARCH_FLAG="-arch x86_64" ;;
      osx-arm64) ARCH_FLAG="-arch arm64" ;;
    esac
  fi
  # shellcheck disable=SC2086
  "$CC_BIN" -std=c11 -Wall -Wextra $ARCH_FLAG -I"$(smoke_native_path -m "$PKG_DIR/include")" "$SMOKE_DIR/smoke.c" -o "$SMOKE_DIR/smoke" $DL_FLAG >"$SMOKE_DIR/compile.log" 2>&1 || {
    echo "[smoke_capi] FAIL: compile failed:"; sed 's/^/    /' "$SMOKE_DIR/compile.log"; exit 1
  }
  SMOKE_BIN="$SMOKE_DIR/smoke"
fi

echo "[smoke_capi] running ..."
"$SMOKE_BIN" "$LIB_FILE" "$FIXTURE" "$SENTENCE" || {
  echo "[smoke_capi] FAIL: smoke program exited non-zero"
  exit 1
}

echo ""
echo "[smoke_capi] all checks passed"
exit 0
