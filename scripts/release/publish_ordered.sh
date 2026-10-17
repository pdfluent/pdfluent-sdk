#!/usr/bin/env bash
# publish_ordered.sh — publish every publishable workspace crate to crates.io
# in topological dependency order.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md. Read that first; this script
# enforces it, it does not replace it.
#
# Why this exists: `.gitlab-ci.yml` has called this file since the release jobs
# were written, and the file was never in the repository. The only automated
# publish path would have failed in seconds, and nobody would have found out
# until the moment they tried to ship (found 22-08-2026, during a 1.0.0
# attempt). scripts/ci/ci_config_lint.py now fails on a job that calls a script
# that is not there.
#
# Usage:
#   scripts/release/publish_ordered.sh              # dry run, publishes nothing
#   scripts/release/publish_ordered.sh --live       # actually publishes
#   scripts/release/publish_ordered.sh --from NAME  # resume at a crate
#
# Resumable on purpose. A 32-crate publish that dies at crate 19 must not force
# a choice between re-publishing eighteen versions (impossible; crates.io is
# append-only) and hand-editing a list under time pressure. Crates already on
# crates.io at the exact version are skipped with a line saying so.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

LIVE=false
START_AT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --live) LIVE=true; shift ;;
    --from) START_AT="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
    *) echo "publish_ordered: unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Protocol §7: clean tree, and --allow-dirty is banned outright.
#
# The same rule the audit uses, from the same file. This check had its own copy
# of it and did not have the repair, so a RESUMED publish refused over the audit
# reports its own previous run had written -- the first run of a publish starts
# clean, which is why three runs went past it and the fourth did not (#358).
# shellcheck source=scripts/release/tree_dirt.sh
source "${SCRIPT_DIR}/tree_dirt.sh"
DIRTY="$(pdfluent_tree_dirt "${REPO_ROOT}")"
if [ -n "${DIRTY}" ]; then
  echo "publish_ordered: working tree is not clean — PUBLISH_PROTOCOL.md §7" >&2
  echo "${DIRTY}" >&2
  exit 1
fi

# Is <crate>@<version> on crates.io? 0 yes, 1 no, 2 could not tell. Never
# `cargo info` from here: see the rule's own header and the two call sites.
crate_on_registry() {
  python3 "${SCRIPT_DIR}/crate_version_is_on_crates_io.py" "$1" "$2"
}

# Is <crate>@<version> resolvable from the cargo INDEX? A different surface
# from the API above, and the one that matters after an upload: the next crate
# resolves its `=` pin through the index, and crates.io can serve the API
# before the index has caught up.
#
# `--registry crates-io` is what keeps this from becoming the original defect
# again -- a bare `cargo info` inside the workspace answers out of
# ./crates/<name> and never looks at a registry at all. Measured 16-09-2026:
# with the flag, `pdf-invoice@1.0.0` in this workspace reports "could not find
# ... in registry", which is the truth. (codex, #1915)
crate_in_cargo_index() {
  cargo info --registry crates-io "$1@$2" >/dev/null 2>&1
}

echo "[publish] computing topological order via topological_publish_order.py..."
ORDER_FILE="$(mktemp)"
# THE ONE ORDER (#636). This was an inline copy of the same Kahn sort, and a
# second copy of an order is a second order: it counted dev-dependencies,
# which cargo strips from the uploaded manifest, and broke ties differently
# from the topological_publish_order.py the audit, the smoke helper and the
# tests hold. One source, printed `<name> <version>` per line.
python3 "${SCRIPT_DIR}/topological_publish_order.py" | awk '{print $1 "\t" $2}' > "${ORDER_FILE}"

TOTAL="$(wc -l < "${ORDER_FILE}" | tr -d ' ')"
echo "[publish] ${TOTAL} publishable crates"

# NO VERSION IS REUSED WITH OTHER CONTENT (#358).
#
# crates.io is append-only. A crate whose version is already served and whose
# files have moved on either breaks every dependent's `cargo package --verify`
# (which resolves that dependency from the registry, not from the path), or --
# worse, when the API happens to have stayed the same -- publishes and ships a
# user the OLD content out of the registry. Five forks were in that state on
# 16-09-2026 and the run found out at crate 6 of 32, after two were already
# published and could not be taken back.
#
# Up front, over every crate, so the answer arrives before the first upload
# rather than in the middle of the train. Exit 2 is "the registry could not be
# read", which stops the run as well: a publish that cannot check is not a
# publish worth making.
# THE OWNER'S RELEASE RULE, BEFORE THE FIRST UPLOAD (#575). This is the path
# the hosted crates.io workflow actually runs, which is why the gate is here
# and not only in `release_train_guard.sh`. (codex, #1930)
if ! bash "${SCRIPT_DIR}/the_structured_extraction_is_at_parity.sh"; then
  exit 1
fi

echo "[publish] checking no version is already published with other content..."
if ! python3 "${SCRIPT_DIR}/published_version_matches_the_workspace.py" --all; then
  echo "publish_ordered: a version is already on crates.io with different content — stopping" >&2
  exit 1
fi
$LIVE || echo "[publish] DRY RUN — nothing will be published. Pass --live to publish."

SKIPPING=false
[ -n "${START_AT}" ] && SKIPPING=true

# Dry-run collects the crates that pass the checks; the single multi-package
# invocation over this set is after the loop (#636).
DRY_RUN_NAMES=()

i=0
while IFS=$'\t' read -r NAME VERSION; do
  i=$((i + 1))
  if $SKIPPING; then
    if [ "${NAME}" = "${START_AT}" ]; then SKIPPING=false; else
      echo "[publish] ${i}/${TOTAL} ${NAME} — skipped (before --from ${START_AT})"
      continue
    fi
  fi

  # ALREADY ON CRATES.IO? ASK CRATES.IO (#358).
  #
  # This was `cargo info "${NAME}@${VERSION}"`, run from ${REPO_ROOT}. Inside
  # the workspace that resolves to the local path member and never reaches the
  # registry, so every one of the 32 workspace crates answered "yes": --live
  # skipped all of them, never reached `cargo publish` below, and exited 0
  # saying "done". Measured 16-09-2026, when crates.io had `pdfluent` at
  # 1.0.0-beta.18 and had never heard of `pdf-invoice` at all.
  #
  # Exit 2 is "could not tell". Here that is treated as not-published: the
  # publish below is the safe direction, because crates.io refuses a duplicate.
  #
  # `crate_on_registry ...; seen=$?` would be a top-level simple command under
  # `set -euo pipefail`, and 1 -- "not published", the normal case for a crate
  # about to be published -- would end the run right there, before the audit
  # and before `cargo publish`. `if` is one of the contexts errexit exempts.
  # (codex, #1915)
  if crate_on_registry "${NAME}" "${VERSION}"; then
    echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — already on crates.io, skipping"
    continue
  fi

  echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — auditing"
  if ! bash "${SCRIPT_DIR}/prepublish_crate_audit.sh" "${NAME}"; then
    echo "publish_ordered: audit failed for ${NAME} — stopping (PUBLISH_PROTOCOL.md §2)" >&2
    exit 1
  fi

  if ! $LIVE; then
    echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — dry-run publish (queued)"
    DRY_RUN_NAMES+=("${NAME}")
    continue
  fi

  echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — PUBLISHING"
  cargo publish -p "${NAME}"

  # crates.io serves the index a moment after the upload. Publishing the next
  # crate before its dependency is resolvable fails with a message about a
  # version that "does not exist", which reads like a manifest error and is not.
  #
  # The index rather than the API, because what this protects is the NEXT
  # crate's `=` pin, and that resolves through the index. The version of this
  # loop that asked a bare `cargo info` from the workspace confirmed every
  # upload whether or not it happened (#358).
  visible=false
  for _ in $(seq 1 60); do
    if crate_in_cargo_index "${NAME}" "${VERSION}"; then visible=true; break; fi
    sleep 5
  done
  if ! $visible; then
    echo "publish_ordered: ${NAME} ${VERSION} did not become visible on crates.io within 5 minutes — stopping rather than publishing a dependent against a version that may not be there" >&2
    exit 1
  fi
  echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — visible in the index"
done < "${ORDER_FILE}"

# DRY-RUN VERIFIES THE WHOLE SET IN ONE INVOCATION (#636).
#
# A per-crate `cargo publish --dry-run -p NAME` resolves NAME's `=1.1.0`
# workspace pins from crates.io, where they do not exist yet before the
# release -- so the preflight failed on every crate with an unpublished
# workspace dependency (exit 101, "failed to select a version for the
# requirement `pdf-annot ="=1.1.0"`"). Selected together, cargo 1.94
# resolves those versions through its local overlay instead.
#
# `--no-verify`, because the verifying form of a multi-package publish ICEs
# on 1.94 ("no hash listed for <crate> v1.1.0" in its hashless tmp-registry
# -- the same ICE `verify_packaged_workspace.sh` exists for). The build the
# verify would have done is not lost: every crate above passed
# `prepublish_crate_audit.sh`, whose workspace route builds the packaged
# crates standalone. When cargo fixes the ICE the flag can go.
#
# An explicit `-p` list rather than `--workspace`: the set is what this run
# checked and audited -- already-published crates and `--from` skips stay
# out -- in the topological order above. The live branch in the loop is
# untouched: crates.io is append-only, so a release still publishes one
# crate at a time, each dependency before its dependents.
if ! $LIVE; then
  if [ "${#DRY_RUN_NAMES[@]}" -eq 0 ]; then
    echo "[publish] every crate is already on crates.io — nothing to dry-run"
  else
    echo "[publish] dry-running ${#DRY_RUN_NAMES[@]} crates in one multi-package invocation"
    DRY_RUN_ARGS=()
    for DRY_NAME in "${DRY_RUN_NAMES[@]}"; do
      DRY_RUN_ARGS+=(-p "${DRY_NAME}")
    done
    cargo publish --dry-run --no-verify "${DRY_RUN_ARGS[@]}"
  fi
fi

rm -f "${ORDER_FILE}"
echo "[publish] done ($($LIVE && echo live || echo "dry run"))"
