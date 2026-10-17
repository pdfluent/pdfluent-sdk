#!/usr/bin/env bash
# registry_dry_run.sh -- build every registry artefact of one release locally,
# publish nothing, and write down what each registry would receive.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md (rule 11: a dry run on every
# channel that supports one) and by docs/release/REGISTRY_1_0_0_RUNBOOK.md,
# which is the operator's page for the publish itself.
#
# WHY THIS EXISTS
#
# The six registry artefacts are built by six toolchains, and before this
# script the only way to know whether all six still build at the version in
# the manifests was to run six commands from six different runbook sections
# and remember what each of them printed. That is how a release goes out with
# five channels at one version and the sixth at the previous one.
#
# It never publishes. There is no flag that makes it publish.
#
# Usage:
#   scripts/release/registry_dry_run.sh [--out DIR] [--only crates,pypi,...]
#
# Channels: crates, pypi, npm-wasm, npm-node, nuget, maven
#
# Exit codes:
#   0  every requested channel produced an artefact
#   1  at least one did not (the report says which, and why)
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="${REPO_ROOT}/benchmarks/runs/registry_dry_run/$(date -u +%Y%m%d)"
ONLY="crates,pypi,npm-wasm,npm-node,nuget,maven"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out)  OUT_DIR="$2"; shift 2 ;;
        --only) ONLY="$2"; shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "$0" | head -28; exit 0 ;;
        *) echo "unknown flag: $1" >&2; exit 2 ;;
    esac
done

mkdir -p "$OUT_DIR"
REPORT="${OUT_DIR}/report.md"
: > "$REPORT"

# The one version every channel must name. Read from the workspace manifest so
# this script cannot disagree with the guard that holds the six manifests
# together (scripts/ci/every_registry_names_the_same_release.py).
VERSION="$(python3 - <<'PY'
import pathlib, re
text = pathlib.Path("Cargo.toml").read_text(encoding="utf-8")
block = text.split("[workspace.package]", 1)[1]
print(re.search(r'^version\s*=\s*"([^"]+)"', block, re.M).group(1))
PY
)"

echo "# Registry dry run -- ${VERSION}" >> "$REPORT"
echo >> "$REPORT"
# The host triple, not `uname -m`. A shell started under Rosetta reports
# x86_64 on an M1, while cargo -- which is what builds the artefacts --
# targets arm64. A report naming the wrong architecture is worse than one
# naming none: the .nupkg says osx-arm64 either way.
HOST="$(rustc -vV | sed -n 's/^host: //p')"
echo "Built $(date -u +%Y-%m-%dT%H:%M:%SZ) for ${HOST}. Nothing was published." >> "$REPORT"
echo >> "$REPORT"
echo "| channel | package | result | artefact | bytes |" >> "$REPORT"
echo "|---|---|---|---|---|" >> "$REPORT"

FAILED=0

wants() { [[ ",${ONLY}," == *",$1,"* ]]; }

# What is inside an artefact, whatever kind of archive it is.
members() {
    case "$1" in
        *.crate|*.tgz|*.tar.gz) tar tzf "$1" 2>/dev/null ;;
        *.whl|*.nupkg|*.jar)    unzip -Z1 "$1" 2>/dev/null ;;
    esac
}

# Record one channel's outcome. A channel that produced no artefact is a
# failure even when its command exited zero: an empty dry run reports OK over
# nothing, which is the one answer a dry run may never give.
#
# The fifth argument is an extended regular expression every artefact of that
# channel must match at least once -- the licence texts its manifest promises
# (PUBLISH_PROTOCOL.md rule 7). Every published @pdfluent/sdk-wasm up to
# 1.0.0-beta.17.5 declared `SEE LICENSE IN LICENSE-OFFER` and shipped no such
# file, because the manifest was audited and the tarball was not.
record() {
    local channel="$1" package="$2" status="$3" artefact="$4" required="${5:-}"
    local bytes="--"
    if [[ -n "$artefact" && -f "$artefact" ]]; then
        bytes="$(wc -c < "$artefact" | tr -d ' ')"
        if [[ -n "$required" ]] && ! members "$artefact" | grep -qE "$required"; then
            status="built, but nothing in it matches ${required}"
        fi
    elif [[ "$status" == "ok" ]]; then
        status="no artefact"
    fi
    [[ "$status" == "ok" ]] || FAILED=1
    echo "| ${channel} | ${package} | ${status} | ${artefact:-none} | ${bytes} |" >> "$REPORT"
    printf '%-10s %-24s %s\n' "$channel" "$package" "$status"
}

# Append, never truncate: a channel that runs two commands must leave both in
# its log, and the second one silently erasing the first is how a failure gets
# read as "no output".
run() {  # run <logfile> <command...>
    local log="$1"; shift
    if "$@" >> "$log" 2>&1; then return 0; fi
    return 1
}

if wants crates; then
    # One question: does every publishable crate build against the PACKAGED
    # versions of its siblings -- not the registry's 1.0.0, not the source
    # tree? That is scripts/release/verify_packaged_workspace.sh, which the
    # prepublish audit's workspace route also calls, so the two cannot drift.
    #
    # It packs with `cargo package --workspace --locked --no-verify` and then
    # builds the unpacked .crates standalone. The verifying
    # `cargo package --workspace --locked` is NOT used here: it ICEs on
    # cargo 1.94 ("no hash listed for xfa-dom-resolver v1.1.0" in its
    # hashless tmp-registry). When cargo fixes the ICE the route can return
    # to plain `cargo package --workspace`.
    #
    # `--allow-dirty` is the dry-run exploration the protocol permits
    # (PUBLISH_PROTOCOL.md rule 7); the publish path itself
    # (`publish_ordered.sh`) refuses a dirty tree.
    LOG="${OUT_DIR}/crates.log"
    : > "$LOG"
    TOTAL="$(python3 scripts/release/topological_publish_order.py | grep -c .)"
    PKGDIR="$(cargo metadata --no-deps --format-version 1 2>/dev/null \
        | python3 -c "import json,sys; print(json.load(sys.stdin).get('target_directory','') + '/package')")"
    HELPER_REPORT="${OUT_DIR}/crates_packaged_build.txt"
    START="$(date +%s)"
    if bash scripts/release/verify_packaged_workspace.sh --allow-dirty \
            --scratch "${OUT_DIR}/synth" --report "${HELPER_REPORT}" >>"$LOG" 2>&1; then
        record crates "${TOTAL}/${TOTAL} crates" ok "${PKGDIR}/pdfluent-${VERSION}.crate"
        echo "" >>"$REPORT"
        echo "All ${TOTAL} publishable crates built against their packaged siblings." >>"$REPORT"
        echo "pdfluent-cli built against packaged pdfluent ${VERSION}" >>"$REPORT"
        echo "" >>"$REPORT"
        echo "Per-crate packaged build (see crates_packaged_build.txt):" >>"$REPORT"
        echo '```' >>"$REPORT"
        cat "${HELPER_REPORT}" >>"$REPORT"
        echo '```' >>"$REPORT"
    else
        record crates "0/${TOTAL} crates" "packaged build failed (see crates.log)" ""
    fi
    END="$(date +%s)"
    echo "crates channel wall time: $((END - START))s" >>"$LOG"
fi

if wants pypi; then
    LOG="${OUT_DIR}/pypi.log"
    : > "$LOG"
    if run "$LOG" maturin build --release -m crates/pdf-python/Cargo.toml --out "${OUT_DIR}/pypi"; then
        WHEEL="$(ls -1 "${OUT_DIR}"/pypi/*.whl 2>/dev/null | head -1)"
        record pypi pdfluent ok "$WHEEL" "licenses/LICENSE-COMMERCIAL$"
        python3 -m twine check "${OUT_DIR}"/pypi/*.whl >> "$LOG" 2>&1 \
            || { echo "twine check failed" >> "$REPORT"; FAILED=1; }
    else
        record pypi pdfluent "failed (see pypi.log)" ""
    fi
fi

if wants npm-wasm; then
    LOG="${OUT_DIR}/npm-wasm.log"
    : > "$LOG"
    # The three licence files are copied in, not built in: wasm-pack emits a
    # package.json whose `license` promises a LICENSE-OFFER that nothing else
    # puts in the directory, and `transform-wasm-pkg.sh` refuses the package
    # without it. .github/workflows/wasm.yml does the same three copies.
    if run "$LOG" wasm-pack build crates/xfa-wasm --release --target web --out-dir pkg \
        && cp LICENSE-OFFER LICENSE LICENSE-COMMERCIAL crates/xfa-wasm/pkg/ \
        && run "$LOG" scripts/release/transform-wasm-pkg.sh crates/xfa-wasm/pkg "$VERSION" \
        && ( cd crates/xfa-wasm/pkg && npm pack --pack-destination "$OUT_DIR" ) >> "$LOG" 2>&1; then
        record npm-wasm "@pdfluent/sdk-wasm" ok \
            "$(ls -1 "${OUT_DIR}"/pdfluent-sdk-wasm-*.tgz 2>/dev/null | head -1)" \
            "^package/LICENSE-OFFER$"
    else
        record npm-wasm "@pdfluent/sdk-wasm" "failed (see npm-wasm.log)" ""
    fi
fi

if wants npm-node; then
    LOG="${OUT_DIR}/npm-node.log"
    : > "$LOG"
    # `npm run build` calls the napi CLI, which lives in the package's own
    # devDependencies -- so the install is part of the build, not a precondition
    # somebody is assumed to have met.
    if ( cd crates/pdf-node && npm install --no-audit --no-fund \
            && npm run build && npm pack --pack-destination "$OUT_DIR" ) > "$LOG" 2>&1; then
        record npm-node "@pdfluent/node" ok \
            "$(ls -1 "${OUT_DIR}"/pdfluent-node-*.tgz 2>/dev/null | head -1)" \
            "^package/LICENSE-OFFER$"
    else
        record npm-node "@pdfluent/node" "failed (see npm-node.log)" ""
    fi
fi

if wants nuget; then
    LOG="${OUT_DIR}/nuget.log"
    : > "$LOG"
    # The native libraries are bundled by `Condition="Exists(...)"`, so a RID
    # that was never staged is left out in silence: the package publishes, and
    # every consumer on that platform gets a DllNotFound at load. Build the host
    # native and stage what is there, then say how many of the seven made it --
    # this machine can only produce one of them, and a dry run that hid that
    # would be describing a package nobody can publish.
    #
    # `dotnet pack` alone packs what a build left behind; on a clean tree that
    # is nothing, and NU5026 names the missing assembly rather than the missing
    # build. Build first, then pack what was built.
    #
    # -p:SkipNativeCheck=true: the csproj refuses to pack below seven RIDs, and
    # this machine holds one. The dry run needs the partial nupkg to COUNT it;
    # the refusal lives in the release lane and the artefact gate, not here.
    if run "$LOG" cargo build --release -p pdf-capi \
        && run "$LOG" scripts/release/stage_dotnet_natives.sh \
        && run "$LOG" dotnet build bindings/dotnet/src/PDFluent/PDFluent.csproj -c Release \
        && run "$LOG" dotnet pack bindings/dotnet/src/PDFluent/PDFluent.csproj \
            -c Release --no-build -o "${OUT_DIR}/nuget" -p:SkipNativeCheck=true; then
        NUPKG="${OUT_DIR}/nuget/PDFluent.${VERSION}.nupkg"
        RIDS="$(members "$NUPKG" | grep -c '^runtimes/.*/native/')"
        if [[ "$RIDS" -eq 7 ]]; then
            record nuget PDFluent ok "$NUPKG" "^LICENSE-OFFER$"
        else
            record nuget PDFluent "built with ${RIDS}/7 native runtimes" "$NUPKG"
        fi
    else
        record nuget PDFluent "failed (see nuget.log)" ""
    fi
fi

if wants maven; then
    LOG="${OUT_DIR}/maven.log"
    : > "$LOG"
    # Homebrew's JDK is not on the default java_home path on this machine.
    if [[ -z "${JAVA_HOME:-}" && -x /usr/local/opt/openjdk/bin/java ]]; then
        export JAVA_HOME=/usr/local/opt/openjdk
    fi
    if ( cd bindings/java && mvn -B -DskipTests package ) > "$LOG" 2>&1; then
        record maven "com.pdfluent:pdfluent" ok \
            "bindings/java/target/pdfluent-${VERSION}.jar"
    else
        record maven "com.pdfluent:pdfluent" "failed (see maven.log)" ""
    fi
fi

echo >> "$REPORT"
echo "Report: $REPORT"
cat "$REPORT"
exit "$FAILED"
