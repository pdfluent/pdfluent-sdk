#!/usr/bin/env bash
# smoke_dotnet.sh — Consumer smoke test for the PDFluent .NET / NuGet package.
#
# Creates a minimal .NET project, references the package from a local .nupkg
# (does not require NuGet.org publish), and extracts a fixture PDF to
# markdown through `PDFluent.PdfDocument` — which loads the platform native
# in its static path. An assembly-load check never touches the native
# library, and this smoke used to be exactly that.
#
# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_dotnet.sh [--artifact PATH] [--nupkg PATH]
#       [--version VERSION] [--platform KEY] [--cross-check]
#       [--fixture PATH] [--sentence TEXT]
#
# Options:
#   --artifact PATH  Path to the .nupkg under test (same as --nupkg; this is what
#                    post_publish_smoke_runner.sh passes, and the smoke tests
#                    EXACTLY this file — never whatever auto-detect finds)
#   --nupkg PATH     Same as --artifact (default: auto-detect under bindings/dotnet;
#                    dev lane only — the release lane names its nupkg)
#   --version VER    Expected package version (default: read from .nupkg filename)
#   --platform KEY   Must name this host; refused otherwise unless --cross-check
#   --cross-check    Run even when --platform names another host
#   --fixture PATH   Fixture PDF to extract (default: fixtures/structured.pdf)
#   --sentence TEXT  Known sentence the markdown must contain (default: Safety rules)
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.
#
# Lanes: in a release lane (PDFLUENT_RELEASE_LANE=1) --artifact is required
# and a missing dotnet CLI is a failure — the release smoke vouches for the
# artefact about to ship, on this host, or it vouches for nothing.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_dotnet]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
NUPKG_PATH=""
EXPECTED_VERSION=""
FIXTURE=""
SENTENCE="Safety rules"
NUPKG_GIVEN=0
STRICT="${PDFLUENT_RELEASE_LANE:-0}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact|--nupkg) NUPKG_PATH="$2"; NUPKG_GIVEN=1; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
        --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
        --fixture) FIXTURE="$2"; shift 2 ;;
        --sentence) SENTENCE="$2"; shift 2 ;;
        *) smoke_unknown_option "$1" ;;
    esac
done

smoke_check_platform

[[ -z "$FIXTURE" ]] && FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"

echo "=== PDFluent .NET consumer smoke test ==="

skip_or_fail() { # $1 = reason
    if [[ "$STRICT" == "1" ]]; then
        echo "❌ $1 (release lane: a skip is a failure)"
        exit 1
    fi
    echo "SKIPPED (not a pass): $1"
    exit 0
}

# A given artefact must exist, naming the path — decided before the dotnet
# check, so a typo fails the same way on every machine (smoke_python.sh
# decides its artefact before its virtualenv for the same reason).
if [[ "$NUPKG_GIVEN" == "1" ]]; then
    if [[ ! -f "$NUPKG_PATH" ]]; then
        echo "❌ no such nupkg: ${NUPKG_PATH}" >&2
        exit 2
    fi
    case "$NUPKG_PATH" in
        *.nupkg) ;;
        *) echo "❌ not a nupkg: ${NUPKG_PATH}" >&2; exit 2 ;;
    esac
elif [[ "$STRICT" == "1" ]]; then
    echo "❌ release lane needs --artifact: auto-detect could vouch for any nupkg in the tree" >&2
    exit 2
fi

if ! command -v dotnet &>/dev/null; then
    skip_or_fail "dotnet CLI not found in PATH"
fi

if [[ ! -f "$FIXTURE" ]]; then
    echo "❌ Fixture not found: ${FIXTURE}"
    exit 1
fi
echo "Fixture: ${FIXTURE}"

# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
unset PDFLUENT_LICENSE_KEY

# Auto-detect .nupkg. `mapfile` is bash 4+; macOS bash 3.2 portable form
# uses a while-read loop into the array — same shape as the rest of the
# release toolkit (sbom-generate.sh §"target crate list").
if [[ -z "$NUPKG_PATH" ]]; then
    candidates=()
    while IFS= read -r line; do candidates+=("$line"); done < <(find "${REPO_ROOT}/bindings/dotnet" -name "*.nupkg" -not -path "*/obj/*" 2>/dev/null | sort -r)
    if [[ ${#candidates[@]} -eq 0 ]]; then
        skip_or_fail "no .nupkg under bindings/dotnet (build first: cd bindings/dotnet/src/PDFluent && dotnet pack -c Release)"
    fi
    NUPKG_PATH="${candidates[0]}"
fi

if [[ ! -f "$NUPKG_PATH" ]]; then
    echo "❌ no such nupkg: ${NUPKG_PATH}" >&2
    exit 2
fi

echo "Nupkg: ${NUPKG_PATH}"

# Extract version from filename if not provided.
if [[ -z "$EXPECTED_VERSION" ]]; then
    EXPECTED_VERSION=$(basename "$NUPKG_PATH" | sed 's/PDFluent\.\(.*\)\.nupkg/\1/' || echo "unknown")
fi
echo "Version: ${EXPECTED_VERSION}"
echo ""

SMOKE_DIR=$(mktemp -d -t pdfluent_dotnet_smoke_XXXXXX)
NUGET_SOURCE="${SMOKE_DIR}/nuget_local"
# nuget.config is read by the native dotnet: on Windows (Git Bash) an MSYS path such as
# /tmp/x is read as <current drive>:\tmp\x, not the temp dir it names (15-10 win-x64:
# "NU1301: The local source 'E:\tmp\...\nuget_local' doesn't exist"). Hand it the native form.
if command -v cygpath >/dev/null 2>&1; then NUGET_SOURCE_NATIVE="$(cygpath -w "$NUGET_SOURCE")"; else NUGET_SOURCE_NATIVE="$NUGET_SOURCE"; fi
# The restore extracts into a packages dir of this smoke alone, never the
# user's global ~/.nuget/packages: NuGet keys that cache by id and version, so
# a PDFluent.<version> extracted by an earlier run would be reused and the
# nupkg under test never opened. Native form for the same reason as above.
PACKAGES_DIR="${SMOKE_DIR}/packages"
if command -v cygpath >/dev/null 2>&1; then PACKAGES_DIR_NATIVE="$(cygpath -w "$PACKAGES_DIR")"; else PACKAGES_DIR_NATIVE="$PACKAGES_DIR"; fi
trap 'rm -rf "${SMOKE_DIR}"' EXIT

# Set up a local NuGet source directory.
mkdir -p "${NUGET_SOURCE}"
cp "$NUPKG_PATH" "${NUGET_SOURCE}/"

# Create a minimal .NET project.
mkdir -p "${SMOKE_DIR}/app"
cat > "${SMOKE_DIR}/app/app.csproj" <<XML
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <Nullable>enable</Nullable>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="PDFluent" Version="${EXPECTED_VERSION}" />
  </ItemGroup>
</Project>
XML

# No reflection, no fallback list: the smoke names the real entry point, opens
# a real PDF, and asserts a known sentence. A nupkg without the platform
# native fails here (DllNotFoundException), and a nupkg without the class
# fails at compile time — both red, both loud.
cat > "${SMOKE_DIR}/app/Program.cs" <<'CS'
// Minimal smoke: extract markdown through the public API and assert a known
// sentence. If the type is not available this fails at compile time; if the
// platform native is missing it fails here at runtime.
using System;
using PDFluent;

var fixture = args.Length > 0 ? args[0] : "fixture.pdf";
var sentence = args.Length > 1 ? args[1] : "Safety rules";
try
{
    using var doc = PdfDocument.Open(fixture);
    var markdown = doc.ExtractMarkdown();
    if (!markdown.Contains(sentence))
    {
        Console.Error.WriteLine($"FAIL: '{sentence}' not in markdown output:");
        Console.Error.WriteLine(markdown.Substring(0, Math.Min(500, markdown.Length)));
        Environment.Exit(1);
    }
    Console.WriteLine($"  extractMarkdown: OK ({markdown.Length} chars, '{sentence}' found)");
    var first = markdown.Split('\n')[0];
    Console.WriteLine($"  first line: {first}");
    Console.WriteLine("PDFluent .NET smoke PASS");
}
catch (Exception ex)
{
    Console.Error.WriteLine($"extraction failed: {ex.GetType().Name}: {ex.Message}");
    Environment.Exit(1);
}
CS
cp "$FIXTURE" "${SMOKE_DIR}/app/fixture.pdf"

# Add nuget.config pointing to the local source.
cat > "${SMOKE_DIR}/app/nuget.config" <<XML
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <add key="local" value="${NUGET_SOURCE_NATIVE}" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
XML

echo "Restoring .NET smoke project into ${PACKAGES_DIR_NATIVE}..."
cd "${SMOKE_DIR}/app"
if ! dotnet restore --nologo --packages "$PACKAGES_DIR_NATIVE" 2>&1; then
    echo ""
    echo "❌ .NET consumer smoke FAIL (restore)"
    exit 1
fi

echo "Building .NET smoke project..."
# --no-restore: an implicit restore here would resolve against the global
# packages folder again, undoing the per-smoke dir above.
if dotnet build -c Release --nologo --no-restore 2>&1; then
    echo ""
    echo "Running .NET smoke..."
    if dotnet run -c Release --no-build -- fixture.pdf "$SENTENCE" 2>&1; then
        echo ""
        echo "✅ .NET consumer smoke PASS"
    else
        echo ""
        echo "❌ .NET consumer smoke FAIL (runtime)"
        exit 1
    fi
else
    echo ""
    echo "❌ .NET consumer smoke FAIL (build)"
    exit 1
fi
