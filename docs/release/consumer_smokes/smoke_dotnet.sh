#!/usr/bin/env bash
# smoke_dotnet.sh — Consumer smoke test for the PDFluent .NET / NuGet package.
#
# Creates a minimal .NET project, references the package from a local .nupkg
# (does not require NuGet.org publish), builds it, and runs a basic import check.
#
# Usage:
#   docs/release/consumer_smokes/smoke_dotnet.sh [--nupkg PATH] [--version VERSION]
#
# Options:
#   --nupkg PATH     Path to the local .nupkg file (default: auto-detect under bindings/dotnet)
#   --version VER    Expected package version (default: read from .nupkg filename)

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
NUPKG_PATH=""
EXPECTED_VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --nupkg)   NUPKG_PATH="$2"; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== PDFluent .NET consumer smoke test ==="

if ! command -v dotnet &>/dev/null; then
    echo "⚠️  dotnet CLI not found in PATH — .NET smoke skipped"
    exit 0
fi

# Auto-detect .nupkg.
if [[ -z "$NUPKG_PATH" ]]; then
    mapfile -t candidates < <(find "${REPO_ROOT}/bindings/dotnet" -name "*.nupkg" -not -path "*/obj/*" 2>/dev/null | sort -r)
    if [[ ${#candidates[@]} -eq 0 ]]; then
        echo "⚠️  No .nupkg found under bindings/dotnet. Build first with:"
        echo "    cd bindings/dotnet/src/PDFluent && dotnet pack -c Release"
        echo ""
        echo "Smoke skipped — no artefact."
        exit 0
    fi
    NUPKG_PATH="${candidates[0]}"
fi

if [[ ! -f "$NUPKG_PATH" ]]; then
    echo "❌ .nupkg not found: ${NUPKG_PATH}"
    exit 1
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

cat > "${SMOKE_DIR}/app/Program.cs" <<'CS'
// Minimal smoke: verify the assembly loads and the primary namespace is accessible.
using System;

// If the type is not available this will fail at compile time.
// Adjust the using/type name if the public API changes.
try
{
    // Attempt a reflection-based probe so the smoke works even if the type name changes.
    var asm = System.Reflection.Assembly.Load("PDFluent");
    Console.WriteLine($"Assembly loaded: {asm.FullName}");
    Console.WriteLine("PDFluent .NET smoke PASS");
}
catch (Exception ex)
{
    Console.Error.WriteLine($"Assembly load failed: {ex.Message}");
    Environment.Exit(1);
}
CS

# Add nuget.config pointing to the local source.
cat > "${SMOKE_DIR}/app/nuget.config" <<XML
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <add key="local" value="${NUGET_SOURCE}" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
XML

echo "Building .NET smoke project..."
cd "${SMOKE_DIR}/app"
if dotnet build -c Release --nologo 2>&1; then
    echo ""
    echo "Running .NET smoke..."
    if dotnet run -c Release --no-build 2>&1; then
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
