#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The keep-check half of selfhosted_setup.sh (#636): the setup functions the
# keep-check test exercises (rust, maturin, dotnet) and the napi version
# probe, plus the helpers they share. selfhosted_setup.sh sources this file;
# the keep-check test sources it too and calls one setup function per case,
# so the push lane reaches only these functions and never the install lines
# in the main script -- a push gate may not reach a compiler.
#
# Sourced, never executed. The version globals (rust_version, rust_targets,
# dotnet_version) come from the sourcing script or the environment.
# shellcheck disable=SC2154 # those globals are assigned by the sourcer
#
# The maturin pip pin lives here beside its install, so the pre-launch
# proof reads this file too:
#   python3 scripts/gcp/check_pip_pins.py scripts/release/_selfhosted_checks.sh --python 3.12
MATURIN_VERSION="1.15.0"
# The default pip; setup_python in the main script repoints it at the
# versioned venv on Linux.
PIP="pip3"

die() { echo "selfhosted_setup: REFUSED: $*" >&2; exit 2; }
log() { echo "selfhosted_setup: $*" >&2; }

on_path() {
  # on_path DIR: DIR joins PATH for the steps after this one.
  if [ -n "${GITHUB_PATH:-}" ]; then
    printf '%s\n' "$1" >> "$GITHUB_PATH"
  else
    log "export PATH=\"$1:\$PATH\""
  fi
  export PATH="$1:$PATH"
}

to_env() {
  # to_env NAME VALUE: NAME is set for the steps after this one.
  if [ -n "${GITHUB_ENV:-}" ]; then
    printf '%s=%s\n' "$1" "$2" >> "$GITHUB_ENV"
  else
    log "export $1=\"$2\""
  fi
  export "$1=$2"
}

need_tool() {
  # need_tool NAME WHAT: refuse naming WHAT when NAME is missing.
  command -v "$1" >/dev/null 2>&1 || die "$1 is missing: $2"
}

fetch() {
  # fetch URL DEST: curl with the flags every download here shares.
  need_tool curl "install curl (the setup downloads every toolchain with it)"
  curl -fsSL --retry 3 --retry-delay 5 -o "$2" "$1"
}

# --- rust: rustup, the toolchain, the targets ------------------------------
setup_rust() {
  # A host C toolchain on Linux: build scripts link with `cc` even for
  # zig/musl targets, and the stock image ships no gcc/cc (run 37451377812
  # died on `linker 'cc' not found`). The startup script installs it on
  # new VMs; this covers older ones. (${OS:-}/${SUDO:-}: the keep-check
  # test sources this file without the main script's globals.)
  if [ "${OS:-}" = "Linux" ]; then
    if command -v cc >/dev/null 2>&1; then
      log "cc $(command -v cc) (already present)"
    else
      command -v apt-get >/dev/null 2>&1 || die "cc is missing and this Linux has no apt-get to install build-essential with"
      log "installing build-essential pkg-config (Rust build scripts link with cc)"
      ${SUDO:-} apt-get update -qq
      ${SUDO:-} apt-get install -y -qq build-essential pkg-config
    fi
  fi
  if ! command -v rustup >/dev/null 2>&1; then
    log "installing rustup"
    tmp="$(mktemp -t rustup-init.XXXXXX)"
    fetch "https://sh.rustup.rs" "$tmp"
    sh "$tmp" -y --profile minimal --default-toolchain none
    rm -f "$tmp"
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  fi
  on_path "$HOME/.cargo/bin"
  # No `grep -q` in a pipeline: under pipefail the producer's EPIPE fails a
  # check that matched, so the setup would reinstall on every run. Capture,
  # then compare the string (#636).
  toolchains="$(rustup toolchain list 2>/dev/null || true)"
  if [[ $'\n'"${toolchains}" != *$'\n'"${rust_version}"* ]]; then
    log "installing rust ${rust_version}"
    rustup toolchain install "$rust_version" --profile minimal -c rustfmt -c clippy
  fi
  rustup default "$rust_version"
  # shellcheck disable=SC2086
  for target in ${rust_targets//,/ }; do
    [ -n "$target" ] || continue
    installed_targets="$(rustup target list --toolchain "$rust_version" --installed 2>/dev/null || true)"
    if [[ $'\n'"${installed_targets}" != *$'\n'"${target}"* ]]; then
      log "adding rust target ${target}"
      rustup target add --toolchain "$rust_version" "$target"
    fi
  done
  log "rustc $(rustc --version | cut -d' ' -f2) / cargo $(cargo --version | cut -d' ' -f2)"
}

# --- maturin: the measured pin -------------------------------------------------
setup_maturin() {
  need_tool python3 "list python in --tools before maturin"
  need_tool "$PIP" "list python in --tools before maturin (it brings the pip this install needs)"
  # No `grep -q` in a pipeline here either: capture, then compare (#636).
  shown="$("$PIP" show maturin 2>/dev/null || true)"
  if [[ $'\n'"${shown}"$'\n' == *$'\n'"Version: ${MATURIN_VERSION}"$'\n'* ]]; then
    log "maturin ${MATURIN_VERSION} (already present)"
    return 0
  fi
  log "installing maturin==${MATURIN_VERSION}"
  "$PIP" install --quiet "maturin==${MATURIN_VERSION}"
  log "maturin $("$PIP" show maturin 2>/dev/null | sed -n 's/^Version: //p')"
}

napi_installed_version() {
  # The globally installed @napi-rs/cli version, or nothing when absent.
  # npm is the source of truth: the 2.x binary has no --version flag
  # ("Unsupported option name"), so probing the binary is always false
  # and every run would reinstall (Windows smoke 3, run 37331673176).
  # npm ls exits non-zero when the package is absent: capture, then
  # parse -- no pipe a miss can fail under `set -euo pipefail` (#636).
  listed="$(npm ls -g @napi-rs/cli --depth=0 --json 2>/dev/null || true)"
  [ -n "$listed" ] || return 0
  node -e 'try { console.log((((JSON.parse(process.argv[1] || "{}") || {}).dependencies || {})["@napi-rs/cli"] || {}).version || "") } catch (e) { }' "$listed" 2>/dev/null || true
}

# --- dotnet: the SDK for the wanted major ---------------------------------------
setup_dotnet() {
  if command -v dotnet >/dev/null 2>&1; then
    # Capture, then compare: no `grep -q` in a pipeline (#636).
    sdks="$(dotnet --list-sdks 2>/dev/null || true)"
    if [[ $'\n'"${sdks}" == *$'\n'"${dotnet_version}."* ]]; then
      log "dotnet $(dotnet --version) (already present)"
      return 0
    fi
  fi
  log "installing dotnet ${dotnet_version}"
  tmp="$(mktemp -t dotnet-install.XXXXXX)"
  fetch "https://dot.net/v1/dotnet-install.sh" "$tmp"
  bash "$tmp" --channel "${dotnet_version}.0" --install-dir "$HOME/.dotnet"
  rm -f "$tmp"
  to_env DOTNET_ROOT "$HOME/.dotnet"
  on_path "$HOME/.dotnet"
  log "dotnet $(dotnet --version)"
}
