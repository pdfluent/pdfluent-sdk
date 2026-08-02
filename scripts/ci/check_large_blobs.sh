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
    STAGED_FILES="$(git diff --name-only --diff-filter=ACMR "${BASE_BRANCH}...HEAD" 2>/dev/null || true)"
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

# Build-output path fragments
BLOCKED_PATHS=(
    "target/"
    "dist/"
    "pkg/"
    "build/"
    "fuzz/target/"
    "node_modules/"
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

    # 2. Path check
    for fragment in "${BLOCKED_PATHS[@]}"; do
        if [[ "${lower}" == *"${fragment}"* ]]; then
            reasons+=("PATH contains '${fragment}'")
            break
        fi
    done

    # 3. Size check (only for files that exist on disk and are tracked)
    if [[ -f "${f}" ]]; then
        size_kb=$(( $(wc -c < "${f}") / 1024 ))
        if (( size_kb > MAX_FILE_SIZE_KB )); then
            reasons+=("SIZE=${size_kb}KB (limit ${MAX_FILE_SIZE_KB}KB)")
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
