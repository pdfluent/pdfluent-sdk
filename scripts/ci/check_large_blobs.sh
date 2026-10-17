#!/usr/bin/env bash
# check_large_blobs.sh — guard against accidental large-binary commits.
#
# Runs in two modes depending on context:
#
#   pre-commit hook  : checks git-staged files (git diff --cached)
#   CI pipeline      : checks files changed relative to merge-base
#                      (git diff origin/master...HEAD)
#
# Exit codes:
#   0 — no violations
#   1 — at least one violation found; details printed to stdout
#
# Configuration (environment overrides):
#   MAX_FILE_SIZE_KB  : largest allowed file in KB  (default: 1024 = 1 MB)
#   BASE_BRANCH       : branch to diff against in CI mode (default: origin/master)
#
# Violations:
#   SIZE     — staged file exceeds MAX_FILE_SIZE_KB
#   EXT      — file has a native binary extension (.dylib, .so, .dll, .node,
#               .rlib, .rmeta, .a, .o, .wasm, .nupkg)
#   PATH     — file is inside a build-output directory (target/, dist/, pkg/,
#               build/, fuzz/target/)
#
# Integration:
#   Pre-commit hook (local): copy or symlink to .git/hooks/pre-commit
#     cp scripts/ci/check_large_blobs.sh .git/hooks/pre-commit
#     chmod +x .git/hooks/pre-commit
#
#   CI (GitLab .gitlab-ci.yml):
#     check-blobs:
#       stage: lint
#       script:
#         - bash scripts/ci/check_large_blobs.sh
#       tags: [pdfluent-vps, shell]
#
# The script never modifies the repository; it is safe to run at any time.

set -euo pipefail

MAX_FILE_SIZE_KB="${MAX_FILE_SIZE_KB:-1024}"
BASE_BRANCH="${BASE_BRANCH:-origin/master}"

TAG="[check_large_blobs]"

echo "${TAG} PDFluent repository blob guard"
echo "${TAG} MAX_FILE_SIZE_KB=${MAX_FILE_SIZE_KB}"

# Determine which files to inspect
if git diff --cached --quiet --exit-code 2>/dev/null; then
    # No staged files — CI mode: diff against merge base
    # A base that cannot be read is a refusal, not an empty change: `|| true`
    # here used to turn a missing ref into "no files to inspect -- PASS", which
    # is the one answer a guard must never give for a check it did not run (#599).
    if ! STAGED_FILES="$(git diff --name-only --diff-filter=ACMR "${BASE_BRANCH}...HEAD" 2>&1)"; then
        echo "${TAG} FAIL — cannot diff against ${BASE_BRANCH}: ${STAGED_FILES}"
        echo "${TAG} NOTHING WAS CHECKED: this is not a verdict on the files."
        exit 1
    fi
    MODE="CI (diff ${BASE_BRANCH}...HEAD)"
else
    # Pre-commit hook mode: staged files only
    STAGED_FILES="$(git diff --cached --name-only --diff-filter=ACMR)"
    MODE="pre-commit (staged)"
fi

echo "${TAG} mode: ${MODE}"

if [[ -z "${STAGED_FILES}" ]]; then
    echo "${TAG} PASS — no files to inspect."
    exit 0
fi

FILE_COUNT="$(echo "${STAGED_FILES}" | wc -l | tr -d ' ')"
echo "${TAG} inspecting ${FILE_COUNT} file(s)"

# Binary extension blocklist (lowercase checked; original path reported)
BLOCKED_EXTS=(
    ".dylib" ".dll" ".so" ".node"
    ".rlib"  ".rmeta"
    ".a"     ".o"
    ".wasm"  ".nupkg"
)

# WHERE A LARGE FILE IS THE POINT RATHER THAN AN ACCIDENT.
#
# The size rule exists for generated code: "a learned model is several MB of
# generated code, and the day one was committed whole is the day the policy
# stops" (#599, which wired this script into the gate after it had sat unrun for
# months). It is not a rule about measured evidence, and on 24-09-2026 it stopped
# a daily regression round from landing: a structured round's run file is 1.8 to
# 2.4 MB by construction -- per-page rows for every tool, plus the tail's
# geometry -- and `structured_parity` reads those files out of git, so committing
# them is what the protocol requires rather than something somebody forgot to
# ignore. Nine such files and the DocLayNet manifest were already on master, so
# the rule as wired failed any change that so much as touched the manifest, which
# every round redraws.
#
# So these paths are exempt FROM THE SIZE CHECK ONLY. The extension and
# build-output checks still apply to them, because a `.dylib` under
# `benchmarks/runs/` is exactly as wrong as anywhere else -- and generated
# code under `crates/` is exempt from nothing, which is what #599 was about:
# a learned model is several MB of generated code. Measured test evidence is
# the other class, and it is exempt below wherever the protocol reads it out
# of git, under `crates/` no less than under `benchmarks/`.
SIZE_EXEMPT_PATHS=(
    "benchmarks/runs/"
    "corpus/structured/"
    # Recorded conformance evidence, 2026-10-10 (r4 landing). tests/compat/
    # expected/ holds the re-recorded answers the compat `check` reads out of
    # git: raw-page dumps of big forms and decoded pixmap payloads run to tens
    # of MB by construction, the way a structured round's run file runs to 2
    # MB. Re-recording needs the pinned reference and the network, so
    # committing them is what the protocol requires, and the size rule --
    # which exists for generated code -- is not a rule about them.
    "tests/compat/expected/"
    # Byte-identical engine regression baselines, same day, same reason. The
    # f1040 goldens under crates/pdf-engine/tests/golden/ are read out of git
    # by tests that compare byte for byte; #599 was generated code, not
    # measured evidence, and this names the test-evidence directory only.
    "crates/pdf-engine/tests/golden/"
)

# Hand-written sources that were already over the limit on master before
# #599 wired this guard in. Each entry pins ONE reviewed version: its exact
# byte size and sha256. The file passes the size check only while it is
# byte for byte that version; any edit, growth or same-size rewrite fails
# until somebody reviews it and updates the entry here, in a commit of its
# own. Nothing here follows "whatever master holds".
#
# 15-10-2026, crates/pdf-manip/src/pdfa_fonts.rs: hand-written, 31,944
# lines and 1,286,290 bytes on master e1f136aec (sha256 f498963e...6e857d).
# The release host-path fix has to edit it: two `env!("CARGO_MANIFEST_DIR")`
# lookups baked the build checkout into every shipped native, and the fix
# puts them behind `not(pdfluent_release)` at four cfg sites (+17/-6 lines,
# +835 bytes: cfg attributes, comments and an empty-base check; no data,
# tables or font content). Reviewed version: 31,955 lines, 1,287,125 bytes.
# This lifts the SIZE check only, for this exact path: the extension, path
# and every other check still apply, and so do the font, licence and
# clean-room checks elsewhere. Splitting the file is the way out.
# Format: "path:bytes:sha256".
SIZE_CAPPED_PATHS=(
    "crates/pdf-manip/src/pdfa_fonts.rs:1287125:0c8a7094ad5e4f128850bc00f87f55b41e2d798b55a5a0e1d1bf168d060a186c"
)

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# Build-output path fragments
BLOCKED_PATHS=(
    "target/"
    "dist/"
    "pkg/"
    "build/"
    "fuzz/target/"
    "node_modules/"
)

# Hand-written sources that live under a build-output fragment.
#
# `crates/pdf-node/scripts/build/` holds hand-written SOURCE, not build
# output: the napi post-build step (`postbuild.cjs` and its templates) that
# re-applies the hand-maintained layers `napi build` regenerates away. It
# matches the `build/` fragment above, and it already exists on master, so
# without this exemption the guard fails every branch that touches it.
# Exact path prefixes, exempt from the PATH check ONLY: the extension and
# size checks still apply to these paths.
PATH_EXEMPT_PREFIXES=(
    "crates/pdf-node/scripts/build/"
    # Case names, not build output, 2026-10-10 (r4 landing). l5-module-hdist
    # and l5-module-vdist are horizontal/vertical-distance compat cases; the
    # `dist/` fragment matches inside `hdist`/`vdist`, and both directories
    # hold small recorded JSON expectations.
    "tests/compat/expected/l5-module-hdist/"
    "tests/compat/expected/l5-module-vdist/"
)

VIOLATIONS=0

while IFS= read -r f; do
    [[ -z "${f}" ]] && continue

    lower="$(echo "${f}" | tr '[:upper:]' '[:lower:]')"
    reasons=()

    # 1. Extension check
    for ext in "${BLOCKED_EXTS[@]}"; do
        if [[ "${lower}" == *"${ext}" ]]; then
            reasons+=("EXT=${ext}")
            break
        fi
    done

    # 2. Path check (skipped for hand-written sources under an exempt
    #    prefix; the extension and size checks still apply to them)
    path_exempt=0
    for prefix in "${PATH_EXEMPT_PREFIXES[@]}"; do
        if [[ "${f}" == "${prefix}"* ]]; then
            path_exempt=1
            break
        fi
    done
    if (( path_exempt == 0 )); then
        for fragment in "${BLOCKED_PATHS[@]}"; do
            if [[ "${lower}" == *"${fragment}"* ]]; then
                reasons+=("PATH contains '${fragment}'")
                break
            fi
        done
    fi

    # 3. Size check (only for files that exist on disk and are tracked)
    size_exempt=0
    for fragment in "${SIZE_EXEMPT_PATHS[@]}"; do
        if [[ "${f}" == "${fragment}"* || "${f}" == *"/${fragment}"* ]]; then
            size_exempt=1
            break
        fi
    done
    cap_bytes=""; cap_sha=""
    for capped in "${SIZE_CAPPED_PATHS[@]}"; do
        IFS=: read -r cap_path cap_max cap_want <<< "${capped}"
        if [[ "${f}" == "${cap_path}" ]]; then
            cap_bytes=${cap_max}; cap_sha=${cap_want}
            break
        fi
    done
    if [[ -f "${f}" ]] && (( size_exempt == 0 )); then
        size_bytes=$(wc -c < "${f}" | tr -d ' ')
        if [[ -n "${cap_bytes}" ]]; then
            if (( size_bytes > cap_bytes )); then
                reasons+=("SIZE=${size_bytes} bytes (reviewed ceiling ${cap_bytes} bytes)")
            elif [[ "$(sha256_of "${f}")" != "${cap_sha}" ]]; then
                reasons+=("SIZE=${size_bytes} bytes, edited since its review (sha256 differs from the pinned version): review it and update its SIZE_CAPPED_PATHS entry, or split the file")
            fi
        else
            size_kb=$(( size_bytes / 1024 ))
            if (( size_kb > MAX_FILE_SIZE_KB )); then
                reasons+=("SIZE=${size_kb}KB (limit ${MAX_FILE_SIZE_KB}KB)")
            fi
        fi
    fi

    if (( ${#reasons[@]} > 0 )); then
        # Join reasons with "; "
        reason_str=""
        for r in "${reasons[@]}"; do
            reason_str="${reason_str:+${reason_str}; }${r}"
        done
        echo "${TAG} VIOLATION: ${f}  [${reason_str}]"
        (( VIOLATIONS++ )) || true
    fi

done <<< "${STAGED_FILES}"

echo ""
if (( VIOLATIONS > 0 )); then
    echo "${TAG} FAIL — ${VIOLATIONS} violation(s) found."
    echo "${TAG}   If intentional, add an exception to .gitignore and re-stage."
    echo "${TAG}   For binary SDK artifacts, the release pipeline handles packaging;"
    echo "${TAG}   never commit them to the source tree."
    exit 1
fi

echo "${TAG} PASS — no violations."
exit 0
