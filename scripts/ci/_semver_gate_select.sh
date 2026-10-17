#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# _semver_gate_select.sh — which crates the semver gate checks (#574).
#
# Sourced by scripts/ci/semver_gate.sh and by
# scripts/ci/test_semver_gate_selection.sh; never executed on its own.
#
# A crate qualifies iff it is in the publish order AND its Cargo.toml at tag
# v1.0.0 was publishable at version 1.0.0 — whatever its CURRENT version is.
# Selecting on the current version silently checks nothing once the workspace
# moves to 1.1.0, which is the failure this file exists to prevent.
#
# All git reads go through `command git`: plain `git` output can be mangled
# by a shell hook.

# Manifest relpath (from the repo root) for a crate name, read from the map
# file $SEMVER_GATE_MANIFEST_MAP (`name=relpath` lines, one per crate). The
# gate builds it from cargo metadata; tests write it by hand.
semver_gate_manifest_for() {
    awk -F= -v name="$1" '
        $1 == name { print substr($0, length($1) + 2); found = 1; exit }
        END { exit !found }
    ' "$SEMVER_GATE_MANIFEST_MAP"
}

# The [workspace.package] version at tag v1.0.0, for crates whose manifest
# says `version.workspace = true`. Memoized: one git read per shell.
SEMVER_GATE_WS_VERSION_CACHE="${SEMVER_GATE_WS_VERSION_CACHE:-}"
semver_gate_workspace_version_at_tag() {
    if [ -z "$SEMVER_GATE_WS_VERSION_CACHE" ]; then
        local root_manifest
        root_manifest="$(command git show v1.0.0:Cargo.toml 2>/dev/null)" || return 1
        SEMVER_GATE_WS_VERSION_CACHE="$(printf '%s\n' "$root_manifest" | sed 's/#.*//' | awk '
            /^\[/ { in_ws = ($0 == "[workspace.package]") }
            in_ws && /^[[:space:]]*version[[:space:]]*=/ {
                gsub(/.*=[[:space:]]*"/, ""); gsub(/".*/, ""); print; exit
            }
        ')"
    fi
    [ -n "$SEMVER_GATE_WS_VERSION_CACHE" ] || return 1
    printf '%s\n' "$SEMVER_GATE_WS_VERSION_CACHE"
}

# True (0) iff <manifest-relpath> existed at tag v1.0.0, was publishable
# there, and carried version 1.0.0 — literally, or inherited from a 1.0.0
# workspace. Only the [package] section is read: dependency tables carry
# their own version keys.
semver_gate_published_at_1_0_0() {
    local manifest="$1" old pkg pub_line ver_line ws_line
    old="$(command git show "v1.0.0:$manifest" 2>/dev/null)" || return 1
    pkg="$(printf '%s\n' "$old" | sed 's/#.*//' | awk '
        /^\[/ { in_pkg = ($0 == "[package]") }
        in_pkg { print }
    ')"
    pub_line="$(printf '%s\n' "$pkg" | grep -E '^[[:space:]]*publish[[:space:]]*=' | head -n 1 | tr -d '[:space:]' || true)"
    if [ "$pub_line" = "publish=false" ] || [ "$pub_line" = "publish=[]" ]; then
        return 1
    fi
    ver_line="$(printf '%s\n' "$pkg" | grep -E '^[[:space:]]*version[[:space:]]*=' | head -n 1 | tr -d '[:space:]"' || true)"
    if [ "$ver_line" = "version=1.0.0" ]; then
        return 0
    fi
    if [ -n "$ver_line" ]; then
        return 1
    fi
    ws_line="$(printf '%s\n' "$pkg" | grep -E '^[[:space:]]*version\.workspace[[:space:]]*=' | head -n 1 | tr -d '[:space:]' || true)"
    [ "$ws_line" = "version.workspace=true" ] || return 1
    [ "$(semver_gate_workspace_version_at_tag)" = "1.0.0" ]
}

# Print the qualifying `name version` lines from an order file. Selection is
# by "published at v1.0.0" (see above); the version printed is the CURRENT
# one, for display only.
semver_gate_select() {
    local name version rest manifest
    while read -r name version rest; do
        [ -n "${name:-}" ] || continue
        manifest="$(semver_gate_manifest_for "$name" 2>/dev/null)" || continue
        [ -n "$manifest" ] || continue
        if semver_gate_published_at_1_0_0 "$manifest"; then
            printf '%s %s\n' "$name" "${version:-?}"
        fi
    done <"$1"
    return 0
}

# Non-default features for the gate's feature pass, from
# scripts/ci/semver_features.txt (`crate: f1 f2` lines; `#` comments and
# blank lines ignored, an inline `#` ends the line). Prints the feature
# list as one space-separated line, or nothing for a crate with no line.
# Always succeeds: the gate validates the file up front (semver_gate.sh)
# and fails closed there, so a lookup here never has to judge it.
# $SEMVER_GATE_FEATURES_FILE overrides the path, for tests.
semver_gate_features_for() {
    awk -v name="$1" '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            n = index(line, ":")
            if (n == 0) { next }
            crate = substr(line, 1, n - 1)
            sub(/[[:space:]]+$/, "", crate)
            if (crate == name) {
                feats = substr(line, n + 1)
                sub(/#.*/, "", feats)
                gsub(/^[[:space:]]+/, "", feats)
                gsub(/[[:space:]]+$/, "", feats)
                gsub(/[[:space:]]+/, " ", feats)
                print feats
                exit
            }
        }
    ' "${SEMVER_GATE_FEATURES_FILE:-scripts/ci/semver_features.txt}" 2>/dev/null || true
}
