#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# release_rustflags.sh — source before a RELEASE artefact build, on any runner.
#
# Every release workflow sources this before every step that compiles Rust, on
# every OS. It computes the --remap-path-prefix block from THIS builder's own
# locations instead of the one machine named in .cargo/config.toml (that file
# keeps its literal block for local dev builds on the Mac only):
#   $HOME                              -> /buildenv (catch-all, first)
#   the repo root                       -> /src
#   ${CARGO_HOME:-$HOME/.cargo}/git     -> /git
#   ${CARGO_HOME:-$HOME/.cargo}/registry -> /registry
#   rustc's sysroot                     -> /rustc
# Last-match-wins, as the config comment explains: the catch-all comes first
# so the specific rules override it. The repo root is $GITHUB_WORKSPACE on CI,
# `git rev-parse --show-toplevel` anywhere else, and this file's own directory
# when there is no git tree (an exported tarball).
#
# Env RUSTFLAGS replace the config's rustflags outright, so this also carries
# the per-target extras the config sets for the target being built, named as
# the optional first argument:
#   wasm32*  -C target-feature=+simd128,+bulk-memory (the WASM SIMD path)
#   others   no extra flag — notably never the musl CRT flag. That flag lives
#            only in musl_cdylib_rustflags.sh, which sources this file for the
#            remaps and appends it: the config cannot tell a cdylib from a
#            binary, and the musl CLI must stay statically linked. Held by
#            test_musl_crt_split.py.
#
# Usage (bash; from anywhere in the checkout — in a subshell so later builds
# are unaffected):
#   ( source scripts/release/release_rustflags.sh [target] && cargo build --release ... )
#
# Windows (Git Bash): every source is emitted in each spelling rustc may have
# recorded — the slash style it arrived in plus the converted drive form — so
# a Unix-style home and a drive-letter workspace map to the same target.
# Sourcing twice is safe: the same rules repeat identically.
_rrf_forms() { # $1 = path: print it plus every alternate spelling, one per line
    local _p="$1" _drive _rest _upper _lower _fwd _back
    printf '%s\n' "$_p"
    case "$_p" in
        /[a-zA-Z]/*) # Git-Bash drive path: the Unix spelling plus both
            # drive-letter spellings (upper drive, the form the OS reports).
            _drive="${_p:1:1}" _rest="${_p:2}"
            _upper="$(printf '%s' "$_drive" | tr '[:lower:]' '[:upper:]')"
            _lower="$(printf '%s' "$_drive" | tr '[:upper:]' '[:lower:]')"
            _back="${_rest//\//\\}"
            printf '%s\n' "/${_lower}${_rest}" "${_upper}:${_rest}" "${_upper}:${_back}"
            ;;
        [a-zA-Z]:*) # Drive-letter path: both slash styles in both drive
            # cases, plus the Unix spelling the shell reports.
            _drive="${_p:0:1}" _rest="${_p:2}"
            _upper="$(printf '%s' "$_drive" | tr '[:lower:]' '[:upper:]')"
            _lower="$(printf '%s' "$_drive" | tr '[:upper:]' '[:lower:]')"
            _fwd="${_rest//\\//}"
            _back="${_rest//\//\\}"
            printf '%s\n' "${_drive}:${_fwd}" "${_drive}:${_back}" \
                "${_upper}:${_fwd}" "${_upper}:${_back}" "/${_lower}${_fwd}"
            ;;
    esac
}
_rrf_junction_real() { # $1 = path: its real Windows path, through any junction
    # The junction is usually an ANCESTOR, not the path itself: the runner's
    # C:\actions-runner-pdfluent\_work is a junction onto E:\runner-work-win,
    # and the NetworkService .cargo onto E:\ns-cargo-home. So every ancestor
    # is asked, in ONE Windows PowerShell 5.1 call (Get-Item .Target; no
    # ResolveLinkTarget before .NET 6), and the deepest link found replaces
    # its prefix; repeated for a junction whose target holds another one.
    local _w _anc _list _hit _a _t _round=0 _orig
    _w="$(cygpath -w "$1" 2>/dev/null || true)"
    [ -n "$_w" ] || return 0
    _orig="$_w"
    while [ "$_round" -lt 4 ]; do
        _round=$((_round + 1))
        _list="" _anc="$_w"
        while :; do
            _list="${_list:+$_list,}'${_anc//\'/\'\'}'"
            case "$_anc" in *\\*) ;; *) break ;; esac
            _anc="${_anc%\\*}"
            case "$_anc" in ?:|'') break ;; esac
        done
        _hit="$(powershell.exe -NoProfile -NonInteractive -Command \
            "foreach (\$a in @($_list)) { \$i = Get-Item -LiteralPath \$a -Force -ErrorAction SilentlyContinue; if (\$i -and \$i.LinkType -and \$i.Target) { \$a + '|' + @(\$i.Target)[0]; break } }" \
            2>/dev/null | tr -d '\r' | head -1 || true)"
        [ -n "$_hit" ] || break
        _a="${_hit%%|*}" _t="${_hit#*|}"
        _t="${_t#\\\\\?\?\\}"   # strip a \??\ NT prefix if one is reported
        case "$_t" in [a-zA-Z]:*) ;; *) break ;; esac  # relative: leave it
        _w="${_t%\\}${_w#"$_a"}"
    done
    [ "$_w" != "$_orig" ] && printf '%s\n' "$_w"
    return 0
}
_rrf_physical() { # $1 = path: print where it really is, when that differs
    # A symlinked or junctioned directory reaches rustc under whichever
    # spelling the path was resolved through: the Mac's ~/.cargo/registry is
    # a symlink onto the build volume, and the Windows runner's workspace
    # and cargo home sit under junctions onto E:. Map the real location too,
    # so either spelling remaps. pwd -P resolves every ancestor's symlink.
    local _p="$1" _phys=""
    [ -d "$_p" ] || return 0
    _phys="$(cd "$_p" 2>/dev/null && pwd -P || true)"
    [ -n "$_phys" ] && [ "$_phys" != "$_p" ] && printf '%s\n' "$_phys"
    # Git Bash's pwd -P does not see through an NTFS junction; ask Windows.
    # cygpath exists only under MSYS/Cygwin, which is where junctions live.
    if command -v cygpath >/dev/null 2>&1 && command -v powershell.exe >/dev/null 2>&1; then
        _rrf_junction_real "$_p"
    fi
    return 0 # sourced under set -e: a path with no other spelling is fine
}
_rrf_add() { # $1 = source path, $2 = target: append each spelling of the rule
    local _src="$1" _tgt="$2" _forms _f _real
    _forms="$(_rrf_forms "$_src")"
    _real="$(_rrf_physical "$_src")"
    if [ -n "$_real" ]; then
        _forms="$_forms
$(printf '%s\n' "$_real" | while IFS= read -r _f; do [ -n "$_f" ] && _rrf_forms "$_f"; done)"
    fi
    while IFS= read -r _f; do
        [ -n "$_f" ] || continue
        case " ${_rrf_remaps} " in
            *" --remap-path-prefix ${_f}=${_tgt} "*) continue ;;
        esac
        _rrf_remaps="${_rrf_remaps:+$_rrf_remaps }--remap-path-prefix ${_f}=${_tgt}"
    done <<_RRF_EOF
$_forms
_RRF_EOF
}
_rrf_remaps=""
_rrf_home="${HOME:-}"
_rrf_repo=""
if [ -n "${GITHUB_WORKSPACE:-}" ]; then
    _rrf_repo="$GITHUB_WORKSPACE"
elif command -v git >/dev/null 2>&1; then
    # safe.directory='*': a CI/gate checkout owned by another user must
    # still yield its root from a scrubbed env without HOME config; this
    # is a read-only path query, and the flag travels on the command line.
    # env -u GIT_*: the pre-push hook exports GIT_DIR, and with GIT_DIR set
    # and no GIT_WORK_TREE `rev-parse --show-toplevel` answers the current
    # directory instead of the root — from a subdirectory the helper then
    # mapped that subdirectory to /src. Drop the variables that steer path
    # resolution (the same trap scripts/ci/_a_fixture_inherits_no_git_env.sh
    # seals shell fixtures against; it unsets every GIT_*, here only the
    # path-steering ones matter for this read-only toplevel query).
    _rrf_repo="$(env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_PREFIX -u GIT_COMMON_DIR git -c safe.directory='*' rev-parse --show-toplevel 2>/dev/null || true)"
    # Physical spelling, so the git route and the fallback below agree
    # even under a symlinked parent (/tmp -> /private/tmp on macOS).
    [ -n "$_rrf_repo" ] && _rrf_repo="$(cd "$_rrf_repo" 2>/dev/null && pwd -P)"
fi
if [ -z "$_rrf_repo" ]; then
    _rrf_repo="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." 2>/dev/null && pwd -P || true)"
fi
_rrf_cargo=""
if [ -n "${CARGO_HOME:-}" ]; then
    _rrf_cargo="$CARGO_HOME"
elif [ -n "$_rrf_home" ]; then
    _rrf_cargo="$_rrf_home/.cargo"
fi
_rrf_sysroot=""
if command -v rustc >/dev/null 2>&1; then
    _rrf_sysroot="$(rustc --print sysroot 2>/dev/null || true)"
fi
[ -n "$_rrf_home" ] && _rrf_add "$_rrf_home" "/buildenv"
[ -n "${USERPROFILE:-}" ] && _rrf_add "$USERPROFILE" "/buildenv"
[ -n "$_rrf_repo" ] && _rrf_add "$_rrf_repo" "/src"
if [ -n "$_rrf_cargo" ]; then
    _rrf_add "$_rrf_cargo/git" "/git"
    _rrf_add "$_rrf_cargo/registry" "/registry"
fi
[ -n "$_rrf_sysroot" ] && _rrf_add "$_rrf_sysroot" "/rustc"
_rrf_extra=""
case "${1:-}" in
    wasm32*) _rrf_extra="-C target-feature=+simd128,+bulk-memory" ;;
esac
# C and C++ sources (cc-rs: rquickjs's quickjs.c, zstd, ...) never see
# --remap-path-prefix: their __FILE__ and debug paths need the compiler's own
# -ffile-prefix-map, for the same pairs in the same last-match-wins order
# (gcc and clang, zig cc included). Appended to CFLAGS/CXXFLAGS and to every
# target- or host-specific variant already set, because cc-rs reads the most
# specific one it finds and ignores plain CFLAGS then. NOT for MSVC: cl.exe
# has no prefix map (and warns D9002 on the flag), so under MSYS/Cygwin it is
# skipped unless the named target is windows-gnu. A pair holding a space is
# skipped too: cc-rs splits these variables on whitespace.
_rrf_cmap=""
case "${1:-}" in
    *windows-gnu*) _rrf_cc_ok=1 ;;
    *) if command -v cygpath >/dev/null 2>&1; then _rrf_cc_ok=0; else _rrf_cc_ok=1; fi ;;
esac
if [ "$_rrf_cc_ok" = 1 ] && [ -n "$_rrf_remaps" ]; then
    _rrf_prev=""
    for _rrf_tok in $_rrf_remaps; do
        if [ "$_rrf_prev" = "--remap-path-prefix" ]; then
            _rrf_cmap="${_rrf_cmap:+$_rrf_cmap }-ffile-prefix-map=${_rrf_tok}"
        fi
        _rrf_prev="$_rrf_tok"
    done
fi
# The compilers cc-rs will call here must accept the flag: gcc only has
# -ffile-prefix-map from GCC 8, and the manylinux2014 aarch64 cross image
# ships gcc 4.8.5, which rejects it and fails the build (15-10). Each
# compiler named by TARGET_CC, HOST_CC, CC or a CC_<target> variable, plus
# `cc`, is asked once. If any of them rejects -ffile-prefix-map the map falls
# back to -fdebug-prefix-map (GCC 4.x and every clang): debug paths are
# still remapped, __FILE__ is not, and the host-path check after the build
# is what judges the artefact. The fallback is reported, never silent.
_rrf_probe_cc() { # $1 = flag; 0 when every compiler found accepts it
    local _cc _seen="" _any=0
    for _cc in "${TARGET_CC:-}" "${HOST_CC:-}" "${CC:-}" $(env | sed -n 's/^CC_[A-Za-z0-9_]*=//p') cc; do
        [ -n "$_cc" ] || continue
        case " $_seen " in *" $_cc "*) continue ;; esac
        _seen="$_seen $_cc"
        # shellcheck disable=SC2086  # a compiler may be "zig cc"
        command -v ${_cc%% *} >/dev/null 2>&1 || continue
        _any=1
        # shellcheck disable=SC2086
        if ! printf 'int x;\n' | $_cc "$1" -x c -c -o /dev/null - >/dev/null 2>&1; then
            _rrf_probe_failed="$_cc"; return 1
        fi
    done
    [ "$_any" = 1 ] || return 0
}
if [ -n "$_rrf_cmap" ] && ! _rrf_probe_cc "-ffile-prefix-map=/a=/b"; then
    echo "release_rustflags: ${_rrf_probe_failed} rejects -ffile-prefix-map; C sources get -fdebug-prefix-map (debug paths remapped, __FILE__ not; the host-path check decides)" >&2
    _rrf_cmap=$(printf '%s' "$_rrf_cmap" | sed 's/-ffile-prefix-map=/-fdebug-prefix-map=/g')
fi
unset -f _rrf_probe_cc; unset _rrf_probe_failed
# MSVC (cl.exe) has no prefix map, but it has -d1trimfile:<prefix>\, which
# strips that prefix from __FILE__ (cl 19.44 on the release desktop: `t.c`
# instead of `E:\...\t.c`, checked 15-10). Without it pdf_capi.dll named the
# runner's checkout through rquickjs's quickjs.c. Only the backslash drive
# spellings are passed: those are what cc-rs hands cl. Written with `-`, not
# `/`, so MSYS never rewrites the value as a path when it reaches cl.
if [ "$_rrf_cc_ok" = 0 ] && [ -n "$_rrf_remaps" ]; then
    _rrf_prev=""
    for _rrf_tok in $_rrf_remaps; do
        if [ "$_rrf_prev" = "--remap-path-prefix" ]; then
            _rrf_from=${_rrf_tok%=*}
            case "$_rrf_from" in
                [A-Za-z]:\\*) case " $_rrf_cmap " in *" -d1trimfile:${_rrf_from}\\ "*) ;; *)
                    _rrf_cmap="${_rrf_cmap:+$_rrf_cmap }-d1trimfile:${_rrf_from}\\" ;; esac ;;
            esac
        fi
        _rrf_prev="$_rrf_tok"
    done
    unset _rrf_from
    # cl.exe also reads its options from the CL environment variable, on every
    # call. Build scripts may replace CFLAGS outright (rquickjs-sys 0.8.1 sets
    # CFLAGS="/std:c11 /experimental:c11atomics" on MSVC, 15-10: the wheel's
    # quickjs.c kept its full path), so the trim rides CL as well.
    if [ -n "$_rrf_cmap" ]; then
        case " ${CL:-} " in *" $_rrf_cmap "*) ;; *) export CL="${CL:+$CL }$_rrf_cmap" ;; esac
    fi
fi
_rrf_append_cc() { # $1 = variable name: append the map unless already there
    local _cur
    eval "_cur=\${$1:-}"
    case " $_cur " in *" $_rrf_cmap "*) return 0 ;; esac
    export "$1=${_cur:+$_cur }$_rrf_cmap"
}
if [ -n "$_rrf_cmap" ]; then
    for _rrf_v in CFLAGS CXXFLAGS $(env | grep -oE '^((TARGET|HOST)_)?C(XX)?FLAGS(_[A-Za-z0-9_.-]+)?=' | tr -d '='); do
        case "$_rrf_v" in *[!A-Za-z0-9_]*) continue ;; esac  # CFLAGS_x86_64-... not a shell name
        _rrf_append_cc "$_rrf_v"
    done
fi
unset _rrf_cmap _rrf_cc_ok _rrf_prev _rrf_tok _rrf_v
unset -f _rrf_append_cc
# `--cfg pdfluent_release` drops the checkout-only lookups whose `env!`
# paths --remap-path-prefix cannot reach (pdf-manip's CMap and font-pack
# directories): a release artefact must not name the builder's checkout.
# Held by scripts/release/test_binary_host_paths.py.
[ -n "$_rrf_remaps" ] && _rrf_remaps="$_rrf_remaps --cfg pdfluent_release"
# Only when a block was computed: exporting an empty RUSTFLAGS would replace
# the config's flags with nothing, silently un-scrubbing the build.
if [ -n "$_rrf_remaps" ]; then
    # shellcheck disable=SC2086
    export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }${_rrf_remaps}${_rrf_extra:+ ${_rrf_extra}}"
fi
unset _rrf_remaps _rrf_extra _rrf_home _rrf_repo _rrf_cargo _rrf_sysroot
unset -f _rrf_add _rrf_forms _rrf_physical _rrf_junction_real
