#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The toolchain half of the self-hosted release runners (#636), Linux and
# macOS. Called by .github/actions/setup-selfhosted with the tools one job
# needs; the Windows half is selfhosted_setup.ps1.
#
#   bash scripts/release/selfhosted_setup.sh --tools rust,python,maturin \
#       --rust-version 1.94.0 --rust-targets aarch64-unknown-linux-gnu \
#       --node-version 20 --python-version 3.12 --maturin-version 1.15.0 \
#       --java-version 17 --dotnet-version 8 --napi-version 2.18.0
#
# Every tool is pinned to the version the release workflows already use
# (node excepted: the workflows use 24 and pass it explicitly; the bare
# default below is the #636 pin for standalone runs). tools is a subset of
# rust node python maturin java dotnet wasm napi zig docker, installed in
# dependency order regardless of the order named.
#
# IDEMPOTENT: a tool already present at the wanted version is kept and only
# its version is logged. Every tool logs its version when it is done, so a
# run shows what the job built with. curl, unzip and ca-certificates are
# ensured on Linux first: the installers below need them.
#
# On GitHub the new bins are appended to GITHUB_PATH (and JAVA_HOME to
# GITHUB_ENV) so the steps after the setup see them; outside a run the
# export lines are printed instead.
#
# The pip installs here and in _selfhosted_checks.sh follow the
# check_pip_pins.py pattern (*_VERSION pins, "$PIP" install, one pinned
# requirement per package), the same as the GPU startups; prove they
# resolve before they bill a VM:
#   python3 scripts/gcp/check_pip_pins.py scripts/release/selfhosted_setup.sh --python 3.12
#   python3 scripts/gcp/check_pip_pins.py scripts/release/_selfhosted_checks.sh --python 3.12
set -euo pipefail

# The pip pins check_pip_pins.py reads (each on its own line, so the
# *_VERSION scan finds them). The maturin pin lives in
# _selfhosted_checks.sh beside its install.
CARGO_ZIGBUILD_VERSION="0.23.4"
ZIGLANG_VERSION="0.16.0"

# python-build-standalone, one release for the Linux minors a distro has no
# package for. Pinned by tag, each build by its sha256 beside the URL below.
# (Deliberately no *_VERSION name: those are pip pins for check_pip_pins.py,
# and this is not a pip package.)
PYTHON_STANDALONE_TAG="20261003"

tools="" rust_version="1.94.0" rust_targets="" node_version="20"
python_version="3.12" java_version="17"
dotnet_version="8" napi_version="2.18.0" maven_version="3.9.9"

# The keep-check functions (setup_rust, setup_maturin, setup_dotnet) and the
# napi version probe, plus the helpers they share, live beside this file,
# so the keep-check test sources them without reaching the install lines
# below -- the push lane refuses any gate that can reach a compiler (#636).
# shellcheck source=scripts/release/_selfhosted_checks.sh disable=SC1091
. "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)/_selfhosted_checks.sh"

while [ $# -gt 0 ]; do
  # shellcheck disable=SC2034  # rust/maturin/dotnet versions are read by _selfhosted_checks.sh
  case "$1" in
    --tools) tools="$2"; shift 2 ;;
    --rust-version) rust_version="$2"; shift 2 ;;
    --rust-targets) rust_targets="$2"; shift 2 ;;
    --node-version) node_version="$2"; shift 2 ;;
    --python-version) python_version="$2"; shift 2 ;;
    --maturin-version) MATURIN_VERSION="$2"; shift 2 ;;
    --java-version) java_version="$2"; shift 2 ;;
    --dotnet-version) dotnet_version="$2"; shift 2 ;;
    --napi-version) napi_version="$2"; shift 2 ;;
    --maven-version) maven_version="$2"; shift 2 ;;
    --help|-h)
      sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) die "unknown argument $1" ;;
  esac
done

[ -n "$tools" ] || die "--tools is required (a subset of rust node python maturin java dotnet wasm napi zig docker)"

# Name anything unknown before installing anything.
KNOWN=",rust,node,python,maturin,java,dotnet,wasm,napi,zig,docker,"
# shellcheck disable=SC2086
for _name in ${tools//,/ }; do
  case "$KNOWN" in
    *,"$_name,"*) ;;
    *) die "unknown tool ${_name} (want a subset of rust node python maturin java dotnet wasm napi zig docker)" ;;
  esac
done

want() {
  case ",${tools}," in *,"$1,"*) return 0 ;; *) return 1 ;; esac
}

OS="$(uname -s)"
ARCH="$(uname -m)"
SUDO=""
if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then SUDO="sudo"; fi

apt_get() {
  # apt_get ...: apt-get the way the runners allow it -- bare as root,
  # through passwordless sudo otherwise. sudo -n fails instead of asking
  # for a password, so a host without it refuses rather than hangs (#636).
  if [ "$(id -u)" -eq 0 ]; then
    apt-get "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo -n apt-get "$@"
  else
    return 99
  fi
}

sha256_of() {
  # sha256_of FILE: the file's sha256, through whichever tool is here.
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    need_tool shasum "install shasum (the python download is checksummed with it)"
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# --- tools an earlier run left behind -----------------------------------------
# A second setup step starts with a fresh PATH; without this it refuses the
# tools an earlier step installed ("cargo is missing", "node is missing").
# So on Linux every existing tool dir joins PATH before the prerequisite
# checks, for this run and (through GITHUB_PATH) the steps after it (#636).
if [ "$OS" = "Linux" ]; then
  for _dir in "$HOME/.cargo/bin" "$HOME/.dotnet"; do
    if [ -d "$_dir" ]; then
      on_path "$_dir"
    fi
  done
  for _dir in "$HOME"/tools/node-*/bin "$HOME"/tools/py*/bin \
      "$HOME"/tools/temurin-*/bin "$HOME"/tools/apache-maven-*/bin; do
    if [ -d "$_dir" ]; then
      on_path "$_dir"
    fi
  done
  unset _dir
fi

# --- Darwin: user-level tools under $PDFLUENT_TOOLS, both architectures ------
#
# On Darwin every tool installs into $PDFLUENT_TOOLS -- refused when unset,
# since the runner's .env sets it to its own tool tree -- with no sudo and
# no Homebrew. The table below pins every download with its sha256: each
# download is verified and a mismatch refuses. (Node majors outside the
# table, and exact node versions that are not a table pin, verify against
# the release's own SHASUMS256.txt instead; that path refuses too.)
#
# Both architectures install for python, node, java and dotnet. The arm64
# side joins PATH (plus JAVA_HOME and DOTNET_ROOT); the x64 side is used
# only through scripts/release/rosetta_x64.sh. setup_darwin_wasm/napi/zig
# delegate to the shared installers: with the arm64 side on PATH those
# install arm64-only. Idempotent like the rest: a tool already present at
# the wanted version is kept. Every tool logs its version and its `file`
# architecture. The Linux branches below are untouched by this block.

# The pinned Darwin download table: one release per tool, both arches.
DARWIN_RUSTUP_VERSION="1.29.1"
DARWIN_RUSTUP_ARM64_SHA256="ec1b9233e7f72990ecd8e62063fa7f6c3dfc2bec8e97f88bff165f9100ac696a"
DARWIN_PBS_RELEASE="20261003"
DARWIN_PBS_311_VERSION="3.11.17"
DARWIN_PBS_311_ARM64_SHA256="3663b71c18364eccfbad74c4f21f9f6149e40b07329cd776287410cc1da5d612"
DARWIN_PBS_311_X64_SHA256="4338dc0c2b954f20ca6437406db5b806626c69bae3cafeec43f1b7d57dd72a88"
DARWIN_PBS_312_VERSION="3.12.15"
DARWIN_PBS_312_ARM64_SHA256="316a463172740e71d8dca1f2730784e325f3f720941137b5d674d5801a632213"
DARWIN_PBS_312_X64_SHA256="a8fd7a91852f19b6d959793ef41fad048631ccb2a334a9ecdf573255298f7978"
DARWIN_NODE_20_VERSION="20.20.2"
DARWIN_NODE_20_ARM64_SHA256="466e05f3477c20dfb723054dfebffe55bc74660ee77f612166fca121dacb65b6"
DARWIN_NODE_20_X64_SHA256="8be6f5e4bb128c82774f8a0b8d7a1cc1365a7977d9657cece0ca647b3fe04e61"
DARWIN_NODE_24_VERSION="24.21.0"
DARWIN_NODE_24_ARM64_SHA256="bed7eea5325e1108f32ce5228ddd6a5f0f08a499ee42aa7442aea583702f6057"
DARWIN_NODE_24_X64_SHA256="1462cb3b3046b815cf8ea436d3da450ec1a9f11dac7e5a46b0ada5305d7e8097"
DARWIN_TEMURIN_RELEASE="jdk-17.0.20.1+1"
DARWIN_TEMURIN_VERSION="17.0.20.1"
DARWIN_TEMURIN_ARM64_SHA256="196d13ba5f10414bef7f6a05a9b3f00edacb18ebacef2b99485db9e2ee18f0e8"
DARWIN_TEMURIN_X64_SHA256="c01975da12ed4235250ff891fe8bba73a9e73037d444b269c9d0922b5dbc8e0a"
DARWIN_MAVEN_SHA256="7a9cdf674fc1703d6382f5f330b3d110ea1b512b51f1652846d9e4e8a588d766"
DARWIN_DOTNET_INSTALL_SHA256="082f7685e156738a1b2e2ed8381a621870d4ce8e8c59278034556f05c186eb2e"

darwin_verify() {
  # darwin_verify FILE WANT: refuse unless FILE's sha256 is WANT.
  local file="$1" want="$2" have
  need_tool shasum "install the Xcode command line tools (shasum verifies every Darwin download here)"
  have="$(shasum -a 256 "$file" | cut -d' ' -f1)"
  [ "$have" = "$want" ] || die "sha256 mismatch for ${file} (want ${want}, have ${have}): refusing a download that is not the pinned one"
}

darwin_fetch() {
  # darwin_fetch URL DEST SHA: download URL to DEST, verified against SHA.
  fetch "$1" "$2"
  darwin_verify "$2" "$3"
}

darwin_arch() {
  # darwin_arch BIN: the `file` architecture of BIN on one line.
  file -b "$1" 2>/dev/null || echo "unknown"
}

# --- rust: rustup for the arm64 host, the toolchain, both targets --------------
setup_darwin_rust() {
  if [ -z "${RUSTUP_HOME:-}" ]; then RUSTUP_HOME="$PDFLUENT_TOOLS/rustup"; fi
  if [ -z "${CARGO_HOME:-}" ]; then CARGO_HOME="$PDFLUENT_TOOLS/cargo"; fi
  export RUSTUP_HOME CARGO_HOME
  if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
    log "installing rustup ${DARWIN_RUSTUP_VERSION} (aarch64-apple-darwin)"
    tmp="$(mktemp -t rustup-init.XXXXXX)"
    darwin_fetch "https://static.rust-lang.org/rustup/archive/${DARWIN_RUSTUP_VERSION}/aarch64-apple-darwin/rustup-init" "$tmp" "$DARWIN_RUSTUP_ARM64_SHA256"
    chmod +x "$tmp"
    "$tmp" -y --profile minimal --default-toolchain none --no-modify-path
    rm -f "$tmp"
  fi
  on_path "$CARGO_HOME/bin"
  # Capture, then compare the string, as setup_rust does: no grep in a
  # pipeline under pipefail (#636).
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
  log "rustc $(rustc --version | cut -d' ' -f2) / cargo $(cargo --version | cut -d' ' -f2) ($(darwin_arch "$CARGO_HOME/bin/rustc"))"
}

darwin_install_python_side() {
  # (MINOR PBS_VERSION SIDE PBS_ARCH SHA): one python-build-standalone
  # install_only tarball into $PDFLUENT_TOOLS/python-MINOR-SIDE.
  local minor="$1" pbs_version="$2" side="$3" parch="$4" sha="$5"
  local dest="$PDFLUENT_TOOLS/python-${minor}-${side}"
  if [ -x "$dest/bin/python3" ] && [ "$("$dest/bin/python3" --version 2>&1 | cut -d' ' -f2)" = "$pbs_version" ]; then
    log "python ${pbs_version} (${side}; already present)"
    return 0
  fi
  log "installing python ${pbs_version} (${side})"
  tmp="$(mktemp -d -t pbs.XXXXXX)"
  darwin_fetch "https://github.com/astral-sh/python-build-standalone/releases/download/${DARWIN_PBS_RELEASE}/cpython-${pbs_version}+${DARWIN_PBS_RELEASE}-${parch}-apple-darwin-install_only.tar.gz" "$tmp/python.tar.gz" "$sha"
  rm -rf "$dest"
  mkdir -p "$dest"
  tar -xzf "$tmp/python.tar.gz" -C "$dest" --strip-components=1
  rm -rf "$tmp"
  if ! "$dest/bin/python3" -m pip --version >/dev/null 2>&1; then
    log "installing pip for ${dest}"
    "$dest/bin/python3" -m ensurepip --upgrade
  fi
}

# --- python: the standalone builds for both architectures ----------------------
setup_darwin_python() {
  case "$python_version" in
    3.11) pbs_version="$DARWIN_PBS_311_VERSION"; arm_sha="$DARWIN_PBS_311_ARM64_SHA256"; x64_sha="$DARWIN_PBS_311_X64_SHA256" ;;
    3.12) pbs_version="$DARWIN_PBS_312_VERSION"; arm_sha="$DARWIN_PBS_312_ARM64_SHA256"; x64_sha="$DARWIN_PBS_312_X64_SHA256" ;;
    *) die "python ${python_version} has no Darwin pin (want 3.11 or 3.12 at release ${DARWIN_PBS_RELEASE})" ;;
  esac
  darwin_install_python_side "$python_version" "$pbs_version" "arm64" "aarch64" "$arm_sha"
  darwin_install_python_side "$python_version" "$pbs_version" "x64" "x86_64" "$x64_sha"
  arm="$PDFLUENT_TOOLS/python-${python_version}-arm64"
  x64="$PDFLUENT_TOOLS/python-${python_version}-x64"
  on_path "$arm/bin"
  log "python $("$arm/bin/python3" --version 2>&1 | cut -d' ' -f2) arm64 ($(darwin_arch "$arm/bin/python${python_version}")) / x64 ($(darwin_arch "$x64/bin/python${python_version}"))"
}

# --- maturin: the pin, into the arm64 python -----------------------------------
setup_darwin_maturin() {
  arm="$PDFLUENT_TOOLS/python-${python_version}-arm64"
  [ -x "$arm/bin/python3" ] || die "maturin needs the arm64 python first (list python in --tools before maturin)"
  # The shared install (_selfhosted_checks.sh: the one MATURIN_VERSION pin,
  # the "$PIP" install form check_pip_pins.py reads), into the arm64 python.
  PIP="$arm/bin/pip"
  setup_maturin
}

darwin_install_node_side() {
  # (EXACT SIDE DIST SHA): one nodejs.org tarball into
  # $PDFLUENT_TOOLS/node-SIDE.
  local exact="$1" side="$2" dist="$3" sha="$4"
  local dest="$PDFLUENT_TOOLS/node-${side}"
  log "installing node v${exact} (${dist})"
  tmp="$(mktemp -d -t node-dl.XXXXXX)"
  darwin_fetch "https://nodejs.org/dist/v${exact}/node-v${exact}-${dist}.tar.gz" "$tmp/node.tar.gz" "$sha"
  rm -rf "$dest"
  mkdir -p "$PDFLUENT_TOOLS"
  tar -xzf "$tmp/node.tar.gz" -C "$PDFLUENT_TOOLS"
  mv "$PDFLUENT_TOOLS/node-v${exact}-${dist}" "$dest"
  rm -rf "$tmp"
}

# --- node: the nodejs.org tarballs for both architectures ----------------------
setup_darwin_node() {
  case "$node_version" in
    20) exact="$DARWIN_NODE_20_VERSION"; arm_sha="$DARWIN_NODE_20_ARM64_SHA256"; x64_sha="$DARWIN_NODE_20_X64_SHA256" ;;
    24) exact="$DARWIN_NODE_24_VERSION"; arm_sha="$DARWIN_NODE_24_ARM64_SHA256"; x64_sha="$DARWIN_NODE_24_X64_SHA256" ;;
    *.*.*)
      exact="${node_version#v}"; arm_sha=""; x64_sha="" ;;
    *)
      need_tool python3 "install python3 (the setup reads the node release index with it)"
      major="${node_version#v}"
      index="$(mktemp -t node-index.XXXXXX)"
      fetch "https://nodejs.org/dist/index.json" "$index"
      exact="$(python3 -c "
import json, sys
major = sys.argv[1]
for row in json.load(open(sys.argv[2])):
    v = row['version']
    if v.split('.')[0] == 'v' + major:
        print(v)
        break
" "$major" "$index" | sed 's/^v//')"
      rm -f "$index"
      [ -n "$exact" ] || die "node: no release found for major ${major}"
      arm_sha=""; x64_sha="" ;;
  esac
  # An exact version that is a table pin verifies against the table.
  case "$exact" in
    "$DARWIN_NODE_20_VERSION") arm_sha="$DARWIN_NODE_20_ARM64_SHA256"; x64_sha="$DARWIN_NODE_20_X64_SHA256" ;;
    "$DARWIN_NODE_24_VERSION") arm_sha="$DARWIN_NODE_24_ARM64_SHA256"; x64_sha="$DARWIN_NODE_24_X64_SHA256" ;;
  esac
  log "node ${node_version} resolves to v${exact}"
  arm="$PDFLUENT_TOOLS/node-arm64"
  x64="$PDFLUENT_TOOLS/node-x64"
  if [ -x "$arm/bin/node" ] && [ "$("$arm/bin/node" --version)" = "v${exact}" ] \
      && [ -x "$x64/bin/node" ] && [ "$("$x64/bin/node" --version)" = "v${exact}" ]; then
    log "node v${exact} (both archs; already present)"
    on_path "$arm/bin"
    return 0
  fi
  if [ -z "$arm_sha" ]; then
    log "node v${exact} is not a table pin; verifying against its SHASUMS256.txt"
    sums="$(mktemp -t node-sums.XXXXXX)"
    fetch "https://nodejs.org/dist/v${exact}/SHASUMS256.txt" "$sums"
    arm_sha="$(awk -v f="node-v${exact}-darwin-arm64.tar.gz" '$2 == f { print $1 }' "$sums")"
    x64_sha="$(awk -v f="node-v${exact}-darwin-x64.tar.gz" '$2 == f { print $1 }' "$sums")"
    rm -f "$sums"
    if [ -z "$arm_sha" ] || [ -z "$x64_sha" ]; then
      die "node: v${exact} publishes no darwin SHASUMS entries"
    fi
  fi
  darwin_install_node_side "$exact" "arm64" "darwin-arm64" "$arm_sha"
  darwin_install_node_side "$exact" "x64" "darwin-x64" "$x64_sha"
  on_path "$arm/bin"
  log "node $("$arm/bin/node" --version) / npm $("$arm/bin/npm" --version) (arm64 ($(darwin_arch "$arm/bin/node")) / x64 ($(darwin_arch "$x64/bin/node")))"
}

darwin_install_temurin_side() {
  # (SIDE TEMURIN_ARCH SHA): one Temurin tarball into
  # $PDFLUENT_TOOLS/temurin-17-SIDE.
  local side="$1" tarch="$2" sha="$3"
  local dest="$PDFLUENT_TOOLS/temurin-17-${side}"
  local tag_esc file_ver
  tag_esc="$(printf '%s' "$DARWIN_TEMURIN_RELEASE" | sed 's/+/%2B/')"
  file_ver="$(printf '%s' "$DARWIN_TEMURIN_RELEASE" | sed 's/^jdk-//; s/+/_/')"
  log "installing temurin ${DARWIN_TEMURIN_VERSION} (${side})"
  tmp="$(mktemp -d -t temurin.XXXXXX)"
  darwin_fetch "https://github.com/adoptium/temurin17-binaries/releases/download/${tag_esc}/OpenJDK17U-jdk_${tarch}_mac_hotspot_${file_ver}.tar.gz" "$tmp/jdk.tar.gz" "$sha"
  rm -rf "$dest"
  mkdir -p "$dest"
  # The mac tarball nests the JDK at <jdk>/Contents/Home (the Linux one
  # has it at <jdk>/): strip to Home's contents, like the Temurin
  # container images, so bin/ lands directly under the dest.
  tar -xzf "$tmp/jdk.tar.gz" -C "$dest" --strip-components=3
  rm -rf "$tmp"
  [ -x "$dest/bin/javac" ] || die "temurin ${side}: no bin/javac after extracting (unexpected tarball layout)"
}

# --- java: Temurin 17 for both architectures, plus maven -----------------------
setup_darwin_java() {
  [ "$java_version" = "17" ] \
    || die "java ${java_version} has no Darwin pin (want 17 at ${DARWIN_TEMURIN_RELEASE})"
  [ "$maven_version" = "3.9.9" ] \
    || die "maven ${maven_version} is not the Darwin pin (3.9.9)"
  arm="$PDFLUENT_TOOLS/temurin-17-arm64"
  x64="$PDFLUENT_TOOLS/temurin-17-x64"
  if ! { [ -x "$arm/bin/javac" ] && [ "$("$arm/bin/javac" -version 2>&1 | sed 's/.* //')" = "$DARWIN_TEMURIN_VERSION" ] \
      && [ -x "$x64/bin/javac" ] && [ "$("$x64/bin/javac" -version 2>&1 | sed 's/.* //')" = "$DARWIN_TEMURIN_VERSION" ]; }; then
    darwin_install_temurin_side "arm64" "aarch64" "$DARWIN_TEMURIN_ARM64_SHA256"
    darwin_install_temurin_side "x64" "x64" "$DARWIN_TEMURIN_X64_SHA256"
  else
    log "temurin ${DARWIN_TEMURIN_VERSION} (both archs; already present)"
  fi
  to_env JAVA_HOME "$arm"
  on_path "$arm/bin"
  maven_dest="$PDFLUENT_TOOLS/apache-maven-${maven_version}"
  if [ -x "$maven_dest/bin/mvn" ] && [ "$("$maven_dest/bin/mvn" --version 2>/dev/null | head -1 | sed 's/.*Maven //; s/ .*//')" = "$maven_version" ]; then
    log "maven ${maven_version} (already present)"
  else
    log "installing maven ${maven_version}"
    tmp="$(mktemp -d -t maven.XXXXXX)"
    darwin_fetch "https://archive.apache.org/dist/maven/maven-3/${maven_version}/binaries/apache-maven-${maven_version}-bin.tar.gz" "$tmp/maven.tar.gz" "$DARWIN_MAVEN_SHA256"
    rm -rf "$maven_dest"
    mkdir -p "$PDFLUENT_TOOLS"
    tar -xzf "$tmp/maven.tar.gz" -C "$PDFLUENT_TOOLS"
    rm -rf "$tmp"
  fi
  on_path "$maven_dest/bin"
  log "javac $("$arm/bin/javac" -version 2>&1 | sed 's/.* //') arm64 ($(darwin_arch "$arm/bin/javac")) / x64 ($(darwin_arch "$x64/bin/javac")) / mvn $("$maven_dest/bin/mvn" --version 2>/dev/null | head -1 | sed 's/.*Maven //; s/ .*//')"
}

# --- dotnet: the SDK for both architectures, separate dirs ---------------------
setup_darwin_dotnet() {
  arm="$PDFLUENT_TOOLS/dotnet-arm64"
  x64="$PDFLUENT_TOOLS/dotnet-x64"
  tmp="$(mktemp -t dotnet-install.XXXXXX)"
  darwin_fetch "https://dot.net/v1/dotnet-install.sh" "$tmp" "$DARWIN_DOTNET_INSTALL_SHA256"
  log "installing dotnet ${dotnet_version} (arm64)"
  bash "$tmp" --channel "${dotnet_version}.0" --architecture arm64 --install-dir "$arm" --no-path
  log "installing dotnet ${dotnet_version} (x64)"
  bash "$tmp" --channel "${dotnet_version}.0" --architecture x64 --install-dir "$x64" --no-path
  rm -f "$tmp"
  to_env DOTNET_ROOT "$arm"
  on_path "$arm"
  log "dotnet $("$arm/dotnet" --version) arm64 ($(darwin_arch "$arm/dotnet")) / x64 ($(darwin_arch "$x64/dotnet"))"
}

# --- wasm/napi/zig: the shared installers, arm64 side only ---------------------
# With the arm64 tools on PATH (rust, node, python above), these install
# exactly as on Linux, into and for the arm64 side.
setup_darwin_wasm() { setup_wasm; }
setup_darwin_napi() { setup_napi; }
setup_darwin_zig() { setup_zig; }
setup_darwin_docker() { setup_docker; }

# --- Linux prerequisites -------------------------------------------------
if [ "$OS" = "Linux" ] && command -v apt-get >/dev/null 2>&1; then
  if ! command -v curl >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
    log "installing curl unzip ca-certificates"
    $SUDO apt-get update -qq
    $SUDO apt-get install -y -qq curl unzip ca-certificates
  fi
fi

# setup_rust lives in _selfhosted_checks.sh, sourced above.

node_arch() {
  case "${OS}-${ARCH}" in
    Linux-x86_64) echo "linux-x64" ;;
    Linux-aarch64|Linux-arm64) echo "linux-arm64" ;;
    Darwin-arm64|Darwin-aarch64) echo "darwin-arm64" ;;
    Darwin-x86_64) echo "darwin-x64" ;;
    *) die "node: no prebuilt binary for ${OS}/${ARCH}" ;;
  esac
}

# --- node: the nodejs.org tarball for the wanted major ----------------------
setup_node() {
  major="${node_version%%.*}"
  if command -v node >/dev/null 2>&1; then
    have="$(node --version | sed 's/^v//; s/\..*//')"
    if [ "$have" = "$major" ]; then
      log "node $(node --version) / npm $(npm --version) (already present)"
      return 0
    fi
    log "node major is ${have}, want ${major}; installing beside it"
  fi
  need_tool python3 "install python3 (the setup reads the node release index with it)"
  dist="$(node_arch)"
  index="$(mktemp -t node-index.XXXXXX)"
  fetch "https://nodejs.org/dist/index.json" "$index"
  ver="$(python3 -c "
import json, sys
major = sys.argv[1]
for row in json.load(open(sys.argv[2])):
    v = row['version']
    if v.split('.')[0] == 'v' + major:
        print(v)
        break
" "$major" "$index")"
  rm -f "$index"
  [ -n "$ver" ] || die "node: no release found for major ${major}"
  dest="$HOME/tools/node-${ver}-${dist}"
  mkdir -p "$HOME/tools"
  if [ -e "$dest" ]; then
    log "node ${ver} (${dist}) is already in ${dest}; keeping it"
  else
    log "installing node ${ver} (${dist})"
    tmp="$(mktemp -d -t node-dl.XXXXXX)"
    fetch "https://nodejs.org/dist/${ver}/node-${ver}-${dist}.tar.xz" "$tmp/node.tar.xz"
    # The tarball already extracts to node-<ver>-<dist>/, so extracting
    # straight into tools and moving it onto its own name moves the dir
    # into itself (mv refuses: run 37306831270). Stage it in a temp dir
    # and move the single top-level dir into place instead.
    stage="$(mktemp -d -t node-stage.XXXXXX)"
    tar -xJf "$tmp/node.tar.xz" -C "$stage"
    entries=("$stage"/*)
    [ "${#entries[@]}" -eq 1 ] || die "node: want one top-level dir in the tarball, got ${#entries[@]}"
    [ -d "${entries[0]}" ] || die "node: the tarball top level is not a dir"
    mv "${entries[0]}" "$dest"
    rm -rf "$tmp" "$stage"
  fi
  on_path "$dest/bin"
  log "node $(node --version) / npm $(npm --version)"
}

# --- python: the wanted minor, in a venv under $HOME/tools ----------------------
# Ubuntu 24.04 splits ensurepip and pip out of the base python into the
# python3.X-venv / python3-pip packages, and refuses pip installs into the
# system python anyway (PEP 668). So on Linux the python tools run from a
# dedicated venv, $HOME/tools/py<major><minor>, never the system
# site-packages; maturin and zig then install through the venv's pip (#636).
# The Darwin half is T4's; this change leaves its behaviour alone.
setup_python() {
  case "$python_version" in
    3.11|3.12) ;;
    *) die "python: --python-version is 3.11 or 3.12, not ${python_version}" ;;
  esac
  if command -v "python${python_version}" >/dev/null 2>&1; then
    py="python${python_version}"
  elif python3 -c "import sys; sys.exit(0 if '%s.%s' % sys.version_info[:2] == '${python_version}' else 1)" 2>/dev/null; then
    py="python3"
  else
    py=""
  fi
  if [ -z "$py" ]; then
    if [ "$OS" = "Linux" ]; then
      if command -v apt-get >/dev/null 2>&1; then
        log "installing python ${python_version}"
        $SUDO apt-get update -qq
        # 24.04 ships 3.12 only: when apt has no such minor the standalone
        # build below provides it instead of a failed install.
        if apt-cache show "python${python_version}" >/dev/null 2>&1; then
          $SUDO apt-get install -y -qq "python${python_version}" "python${python_version}-venv"
          py="python${python_version}"
        fi
      fi
      if [ -z "$py" ]; then
        install_standalone_python
        py="$standalone_pybin"
      fi
    elif [ "$OS" = "Darwin" ]; then
      die "python ${python_version} is missing: install the python.org macOS package for it (needs an admin)"
    else
      die "python ${python_version} is missing and this OS has no installer for it here"
    fi
  fi
  if [ "$OS" = "Linux" ]; then
    ensure_linux_venv
    pip_ver="$("$PIP" --version 2>/dev/null | cut -d' ' -f2)"
  else
    if ! "$py" -m pip --version >/dev/null 2>&1; then
      log "installing pip for ${py}"
      "$py" -m ensurepip --upgrade
    fi
    PYBIN="$(command -v "$py")"
    on_path "$(dirname "$PYBIN")"
    pip_ver="$("$py" -m pip --version 2>/dev/null | cut -d' ' -f2)"
  fi
  log "python $("$py" --version 2>&1 | cut -d' ' -f2) / pip ${pip_ver}"
}

ensure_linux_venv() {
  # Debian and Ubuntu keep venv and ensurepip in python3.X-venv: without it
  # the venv below fails with "No module named ensurepip" (#636).
  if ! "$py" -m venv --help >/dev/null 2>&1 || ! "$py" -c "import ensurepip" >/dev/null 2>&1; then
    command -v apt-get >/dev/null 2>&1 || die "python${python_version} has no venv or ensurepip and this Linux has no apt-get: install its venv package and re-run"
    log "installing python${python_version}-venv python3-pip"
    if apt_get update -qq && apt_get install -y -qq "python${python_version}-venv" python3-pip; then
      :
    else
      die "python${python_version}-venv is missing and this host cannot install it (no passwordless sudo): install python${python_version}-venv and python3-pip, then re-run"
    fi
  fi
  venv="$HOME/tools/py${python_version//./}"
  if [ ! -x "$venv/bin/pip" ]; then
    log "creating the python ${python_version} venv"
    rm -rf "$venv"
    "$py" -m venv "$venv"
  fi
  PIP="$venv/bin/pip"
  PYBIN="$venv/bin/python3"
  on_path "$venv/bin"
}

install_standalone_python() {
  # python ${python_version} is not in apt (24.04 ships 3.12 only): the
  # python-build-standalone install lands under $HOME/tools (#636).
  case "${python_version}-${ARCH}" in
    3.11-x86_64) st_ver="3.11.17"; st_cpu="x86_64"; st_sha="aadcba18994cb8f9ee752aceb31d61e04f75b5d900d76fb575c6fbfabadec434" ;;
    3.11-aarch64|3.11-arm64) st_ver="3.11.17"; st_cpu="aarch64"; st_sha="41f16793d33161dcd9bfa2622332b8af870cfdb4540bd1fdc477f9e6b7c58c26" ;;
    3.12-x86_64) st_ver="3.12.15"; st_cpu="x86_64"; st_sha="731af898886c5f821890dc901eca3c651cca8e51fa7308c159d12a1194aeac91" ;;
    3.12-aarch64|3.12-arm64) st_ver="3.12.15"; st_cpu="aarch64"; st_sha="6541297dd1798dec8b98c3ad7492808a5b9d1c126801ceb2011e7754cd20d1ce" ;;
    *) die "python: no standalone build pinned for ${python_version} on ${ARCH}" ;;
  esac
  standalone_url="https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_STANDALONE_TAG}/cpython-${st_ver}+${PYTHON_STANDALONE_TAG}-${st_cpu}-unknown-linux-gnu-install_only_stripped.tar.gz"
  log "installing python ${st_ver} (standalone)"
  tmp="$(mktemp -d -t pystandalone.XXXXXX)"
  fetch "$standalone_url" "$tmp/python.tar.gz"
  have="$(sha256_of "$tmp/python.tar.gz")"
  [ "$have" = "$st_sha" ] || die "python: checksum mismatch for the ${st_ver} standalone build (want ${st_sha}, have ${have})"
  dest="$HOME/tools/python-${st_ver}-${st_cpu}"
  mkdir -p "$HOME/tools"
  rm -rf "$dest"
  mkdir -p "$dest"
  tar -xzf "$tmp/python.tar.gz" -C "$dest" --strip-components=1
  rm -rf "$tmp"
  standalone_pybin="$dest/bin/python${python_version}"
  [ -x "$standalone_pybin" ] || die "python: ${standalone_pybin} is missing from the standalone build"
  log "python $("$standalone_pybin" --version 2>&1 | cut -d' ' -f2) (standalone)"
}

# setup_maturin lives in _selfhosted_checks.sh, sourced above.

# --- java: temurin plus maven ---------------------------------------------------
setup_java() {
  if command -v javac >/dev/null 2>&1; then
    have="$(javac -version 2>&1 | sed 's/.* //; s/\..*//')"
    if [ "$have" = "$java_version" ]; then
      log "javac $(javac -version 2>&1 | sed 's/.* //') (already present)"
    else
      log "javac major is ${have}, want ${java_version}; installing beside it"
      have=""
    fi
  else
    have=""
  fi
  if [ -z "${have:-}" ]; then
    case "${OS}-${ARCH}" in
      Linux-x86_64)plat="linux/x64" ;;
      Linux-aarch64|Linux-arm64) plat="linux/aarch64" ;;
      Darwin-arm64|Darwin-aarch64) plat="mac/aarch64" ;;
      Darwin-x86_64) plat="mac/x64" ;;
      *) die "java: no temurin binary for ${OS}/${ARCH}" ;;
    esac
    log "installing temurin ${java_version} (${plat})"
    tmp="$(mktemp -d -t temurin.XXXXXX)"
    fetch "https://api.adoptium.net/v3/binary/latest/${java_version}/ga/${plat}/jdk/hotspot/normal/eclipse" "$tmp/jdk.tar.gz"
    dest="$HOME/tools/temurin-${java_version}"
    mkdir -p "$HOME/tools"
    rm -rf "$dest"
    mkdir -p "$dest"
    tar -xzf "$tmp/jdk.tar.gz" -C "$dest" --strip-components=1
    rm -rf "$tmp"
    to_env JAVA_HOME "$dest"
    on_path "$dest/bin"
    log "javac $(javac -version 2>&1 | sed 's/.* //')"
  elif [ -z "${JAVA_HOME:-}" ]; then
    # The JDK was already there; point JAVA_HOME at it for the JNI build.
    to_env JAVA_HOME "$(dirname "$(dirname "$(command -v javac)")")"
  fi
  if ! command -v mvn >/dev/null 2>&1; then
    log "installing maven ${maven_version}"
    tmp="$(mktemp -d -t maven.XXXXXX)"
    fetch "https://archive.apache.org/dist/maven/maven-3/${maven_version}/binaries/apache-maven-${maven_version}-bin.tar.gz" "$tmp/maven.tar.gz"
    mkdir -p "$HOME/tools"
    rm -rf "$HOME/tools/apache-maven-${maven_version}"
    tar -xzf "$tmp/maven.tar.gz" -C "$HOME/tools"
    rm -rf "$tmp"
    on_path "$HOME/tools/apache-maven-${maven_version}/bin"
  fi
  log "mvn $(mvn --version 2>/dev/null | head -1 | sed 's/.*Maven //; s/ .*//')"
}

# setup_dotnet lives in _selfhosted_checks.sh, sourced above.

# --- wasm: wasm-pack plus wasm-bindgen-cli at the lock version -------------------
setup_wasm() {
  need_tool cargo "list rust in --tools before wasm"
  if ! command -v wasm-pack >/dev/null 2>&1; then
    log "installing wasm-pack"
    cargo install wasm-pack --locked
  fi
  if [ -f Cargo.lock ]; then
    ver="$(python3 -c 'import re,pathlib; print(re.search(r"\[\[package\]\]\nname = \"wasm-bindgen\"\nversion = \"([^\"]+)\"", pathlib.Path("Cargo.lock").read_text()).group(1))' 2>/dev/null)" \
      || die "wasm: wasm-bindgen not found in Cargo.lock"
  else
    die "wasm: Cargo.lock is missing: run the setup from the repository root"
  fi
  if command -v wasm-bindgen >/dev/null 2>&1 && [ "$(wasm-bindgen --version | cut -d' ' -f2)" = "$ver" ]; then
    log "wasm-bindgen-cli ${ver} (already present)"
  else
    log "installing wasm-bindgen-cli ${ver} (from Cargo.lock)"
    cargo install wasm-bindgen-cli --version "$ver" --locked
  fi
  log "wasm-pack $(wasm-pack --version | cut -d' ' -f2) / wasm-bindgen $(wasm-bindgen --version | cut -d' ' -f2)"
}

# --- napi: the CLI at the repo pin ----------------------------------------------
setup_napi() {
  need_tool node "list node in --tools before napi"
  need_tool npm "list node in --tools before napi"
  pin="$napi_version"
  if [ -f crates/pdf-node/package.json ]; then
    pin="$(node -e "console.log((require('./crates/pdf-node/package.json').devDependencies || {})['@napi-rs/cli'] || '')" | sed 's/^[\^~>=< ]*//')"
    [ -n "$pin" ] || pin="$napi_version"
  fi
  # The keep-check reads the installed version from npm: the 2.x binary has
  # no --version flag, so probing it was always false and every run
  # reinstalled (Windows smoke 3, run 37331673176).
  if [ "$(napi_installed_version)" = "$pin" ]; then
    log "@napi-rs/cli ${pin} (already present)"
    return 0
  fi
  log "installing @napi-rs/cli@${pin}"
  npm install -g "@napi-rs/cli@${pin}"
  have="$(napi_installed_version)"
  log "@napi-rs/cli ${have:-$pin}"
}

# --- zig: cargo-zigbuild plus zig, for the Linux cross legs -----------------------
setup_zig() {
  need_tool python3 "list python in --tools before zig"
  need_tool "$PIP" "list python in --tools before zig (it brings the pip this install needs)"
  if command -v cargo-zigbuild >/dev/null 2>&1 && command -v zig >/dev/null 2>&1; then
    log "cargo-zigbuild $(cargo-zigbuild --version 2>/dev/null | cut -d' ' -f2) / zig $(zig version 2>/dev/null) (already present)"
    return 0
  fi
  log "installing cargo-zigbuild==${CARGO_ZIGBUILD_VERSION} ziglang==${ZIGLANG_VERSION}"
  "$PIP" install --quiet "cargo-zigbuild==${CARGO_ZIGBUILD_VERSION}" "ziglang==${ZIGLANG_VERSION}"
  log "cargo-zigbuild $(cargo-zigbuild --version 2>/dev/null | cut -d' ' -f2) / zig $(zig version 2>/dev/null)"
}

# --- docker: the musl smokes and cross run containers (Linux only) ------------------
setup_docker() {
  if [ "$OS" != "Linux" ]; then
    log "docker is Linux-only here; nothing to do on ${OS}"
    return 0
  fi
  if ! command -v docker >/dev/null 2>&1; then
    command -v apt-get >/dev/null 2>&1 || die "docker is missing and this Linux has no apt-get to install it with"
    log "installing docker"
    $SUDO apt-get update -qq
    $SUDO apt-get install -y -qq docker.io
  fi
  if ! docker info >/dev/null 2>&1; then
    log "starting the docker daemon"
    ($SUDO service docker start >/dev/null 2>&1 || $SUDO systemctl start docker >/dev/null 2>&1) || true
  fi
  if ! docker info >/dev/null 2>&1; then
    # The daemon is up but this user cannot reach it: a group added now
    # never reaches the already-running runner, so no chmod and no sudo
    # wrapper here can fix the job -- the membership must predate the run.
    if [ -n "$SUDO" ] && $SUDO docker info >/dev/null 2>&1; then
      die "docker answers only through sudo: the runner must be in the docker group before the runner process starts; gcp_runner.sh does this on its VMs"
    fi
    die "docker is installed but its daemon does not answer (is this user in the docker group?)"
  fi
  log "docker $(docker --version | sed 's/Docker version //; s/,.*//')"
}

# On Darwin the tools live in the runner's own tree, never system-wide.
if [ "$OS" = "Darwin" ]; then
  [ -n "${PDFLUENT_TOOLS:-}" ] \
    || die "PDFLUENT_TOOLS is unset: on Darwin every tool installs under \$PDFLUENT_TOOLS (the runner's .env sets it)"
fi

for tool in rust node python maturin java dotnet wasm napi zig docker; do
  if want "$tool"; then
    if [ "$OS" = "Darwin" ]; then
      "setup_darwin_${tool}"
    else
      "setup_${tool}"
    fi
  fi
done

log "done: ${tools}"
