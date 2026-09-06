#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Install the Homebrew cask into a clean prefix and take it out again (#192).
#
# WHY A CLEAN PREFIX AND NOT `brew install` ON THIS MACHINE
#
# Installing into the Homebrew that is already here proves that the cask works
# where a tap, a Caskroom and an /Applications full of software already exist.
# That is not the case anyone reports: the first person to run
# `brew install --cask pdfluent/tap/pdfluent` has none of it. So this builds a
# prefix from nothing -- its own Homebrew clone, its own tap, its own Caskroom,
# its own application directory -- installs the cask there, and removes the
# whole prefix afterwards. Nothing outside the temporary directory is written
# to, including the operator's own Homebrew.
#
# The clone is shallow: 30 MB and about two seconds.
#
# WHAT IT PROVES, AND WHAT IT CANNOT
#
# It downloads the artefact the cask names, checks it against the checksum the
# cask carries (Homebrew refuses the install otherwise), mounts it, and asks the
# installed bundle four questions: is it there, does it call itself the version
# the cask claims, is its bundle identifier ours, and does its signature still
# verify. Then it uninstalls and checks the bundle is gone.
#
# It does not launch the application. A window on a machine nobody is watching
# proves nothing, and Gatekeeper's verdict -- which is the part that decides
# whether a stranger can open it at all -- is available from `spctl` without
# running a line of the software's code.
#
#     bash scripts/release/homebrew_cask_clean_prefix_install.sh
#
# It needs the network. It is NOT in the pre-push gate for that reason: a gate
# that fails when a CDN is having a bad afternoon is a gate people rerun until
# it passes. `scripts/ci/every_package_manifest_names_the_same_release.py` is
# the offline half and runs on every push.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CASK="${REPO}/packaging/homebrew/Casks/pdfluent.rb"
RECORD="${REPO}/packaging/desktop-release.toml"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "[cask] SKIPPED (not a pass): a cask installs on macOS only." >&2
  exit 0
fi
for f in "$CASK" "$RECORD"; do
  [ -f "$f" ] || { echo "[cask] FAIL: $f is missing" >&2; exit 1; }
done

# What the cask claims, read from the cask itself. Comparing the installed
# bundle against a number typed into this script would compare it with nothing.
version="$(sed -n 's/^[[:space:]]*version "\(.*\)"/\1/p' "$CASK" | head -1)"
bundle="$(sed -n 's/^[[:space:]]*app "\(.*\)"/\1/p' "$CASK" | head -1)"
bundle_id="$(sed -n 's/^[[:space:]]*bundle_id = "\(.*\)"/\1/p' "$RECORD" | head -1)"
team_id="$(sed -n 's/^[[:space:]]*team_id = "\(.*\)"/\1/p' "$RECORD" | head -1)"
[ -n "$version" ] && [ -n "$bundle" ] && [ -n "$bundle_id" ] && [ -n "$team_id" ] || {
  echo "[cask] FAIL: could not read version, app, bundle_id and team_id" >&2
  exit 1
}

PREFIX="$(mktemp -d "${TMPDIR:-/tmp}/caskprefix.XXXXXX")"
# Homebrew refuses a prefix that lives inside its own temporary directory, and
# on macOS that directory is where mktemp puts things. Giving it one inside the
# prefix turns the relation around and keeps every byte this run writes in one
# place.
export HOMEBREW_TEMP="${PREFIX}/temp"
export HOMEBREW_CACHE="${PREFIX}/cache"
export HOMEBREW_NO_AUTO_UPDATE=1
export HOMEBREW_NO_ANALYTICS=1
export HOMEBREW_NO_ENV_HINTS=1
APPDIR="${PREFIX}/Applications"
export HOMEBREW_CASK_OPTS="--appdir=${APPDIR}"
mkdir -p "$HOMEBREW_TEMP" "$HOMEBREW_CACHE" "$APPDIR"

opruimen() { rm -rf "$PREFIX"; }
trap opruimen EXIT

echo "[cask] prefix: ${PREFIX}"
git clone --depth=1 -q https://github.com/Homebrew/brew "${PREFIX}/brew"
BREW="${PREFIX}/brew/bin/brew"

# A cask has to come from a tap; Homebrew rejects a path. Building the tap here
# is also the only check there is that `packaging/homebrew/` maps onto a tap
# repository the way the tap instructions say it does: Casks/ at the root.
TAP="${PREFIX}/brew/Library/Taps/pdfluent/homebrew-tap"
mkdir -p "${TAP}/Casks"
cp "$CASK" "${TAP}/Casks/"
git -C "$TAP" init -q -b main
git -C "$TAP" add -A
git -C "$TAP" -c user.name=packaging -c user.email=packaging@example.invalid \
    commit -qm "the tap under test"

echo "[cask] installing pdfluent/tap/pdfluent ${version}"
"$BREW" install --cask pdfluent/tap/pdfluent

APP="${APPDIR}/${bundle}"
fouten=0
klacht() { echo "[cask] FAIL: $*" >&2; fouten=$((fouten + 1)); }

[ -d "$APP" ] || klacht "${bundle} is not in the clean application directory"
if [ -d "$APP" ]; then
  geinstalleerd="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
      "${APP}/Contents/Info.plist" 2>/dev/null || echo '')"
  [ "$geinstalleerd" = "$version" ] || \
    klacht "the installed bundle calls itself ${geinstalleerd:-nothing}, the cask says ${version}"
  ident="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
      "${APP}/Contents/Info.plist" 2>/dev/null || echo '')"
  [ "$ident" = "$bundle_id" ] || \
    klacht "the bundle identifier is ${ident:-nothing}, the record says ${bundle_id}"
  codesign --verify --deep --strict "$APP" 2>/dev/null || \
    klacht "the signature of the installed bundle does not verify"
  codesign -dv "$APP" 2>&1 | grep -q "TeamIdentifier=${team_id}" || \
    klacht "the bundle is not signed by team ${team_id}"
  spctl -a -t exec "$APP" >/dev/null 2>&1 || \
    klacht "Gatekeeper does not accept the installed bundle"
fi

# ADVISORY, AND SAID SO. `brew audit` needs a working Ruby toolchain in the
# clean prefix and refuses outright on a machine whose Command Line Tools are
# behind, which is a fact about the machine and not about the cask. Its output
# is worth reading and its exit code is not worth failing on.
echo "[cask] audit (advisory):"
"$BREW" audit --cask pdfluent/tap/pdfluent 2>&1 | sed 's/^/    /' || \
  echo "    (audit could not run here; see the comment in this script)"

echo "[cask] uninstalling"
"$BREW" uninstall --cask pdfluent/tap/pdfluent
[ ! -e "$APP" ] || klacht "${bundle} survived the uninstall"

if [ "$fouten" -ne 0 ]; then
  echo "[cask] FAIL: ${fouten} problem(s) with ${version}." >&2
  exit 1
fi
echo "[cask] OK: ${version} installs into a clean prefix, is signed by ${team_id}, and uninstalls."
