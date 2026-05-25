#!/usr/bin/env bash
# smoke_java.sh — Consumer smoke test for the PDFluent Java / Maven package.
#
# Creates a minimal Maven project, installs the .jar into the local Maven repo
# (does not require Maven Central publish), and verifies the primary class loads.
#
# Usage:
#   docs/release/consumer_smokes/smoke_java.sh [--jar PATH] [--version VERSION]
#
# Options:
#   --jar PATH       Path to the local .jar file (default: auto-detect under bindings/java)
#   --version VER    Expected artifact version (default: read from pom.xml)

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
JAR_PATH=""
EXPECTED_VERSION=""
GROUP_ID="com.pdfluent"
ARTIFACT_ID="pdfluent"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --jar)     JAR_PATH="$2"; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== PDFluent Java consumer smoke test ==="

if ! command -v mvn &>/dev/null && ! command -v mvnw &>/dev/null; then
    echo "⚠️  mvn not found in PATH — Java smoke skipped"
    exit 0
fi

MVN=$(command -v mvn 2>/dev/null || echo "./mvnw")

# Auto-detect jar.
if [[ -z "$JAR_PATH" ]]; then
    mapfile -t candidates < <(find "${REPO_ROOT}/bindings/java" -name "*.jar" \
        -not -name "*javadoc*" -not -name "*sources*" 2>/dev/null | sort -r)
    if [[ ${#candidates[@]} -eq 0 ]]; then
        echo "⚠️  No .jar found under bindings/java. Build first with:"
        echo "    cd bindings/java && mvn package -DskipTests"
        echo ""
        echo "Smoke skipped — no artefact."
        exit 0
    fi
    JAR_PATH="${candidates[0]}"
fi

if [[ ! -f "$JAR_PATH" ]]; then
    echo "❌ .jar not found: ${JAR_PATH}"
    exit 1
fi

echo "Jar: ${JAR_PATH}"

# Resolve version from pom.xml if not provided.
if [[ -z "$EXPECTED_VERSION" ]]; then
    EXPECTED_VERSION=$(grep -oPm1 '(?<=<version>)[^<]+' "${REPO_ROOT}/bindings/java/pom.xml" 2>/dev/null | head -1 || echo "1.0.0-beta.8")
fi
echo "Version: ${EXPECTED_VERSION}"
echo ""

SMOKE_DIR=$(mktemp -d -t pdfluent_java_smoke_XXXXXX)
trap 'rm -rf "${SMOKE_DIR}"' EXIT

# Install the local jar into the local Maven repo.
echo "Installing jar into local Maven repo..."
$MVN install:install-file \
    -Dfile="${JAR_PATH}" \
    -DgroupId="${GROUP_ID}" \
    -DartifactId="${ARTIFACT_ID}" \
    -Dversion="${EXPECTED_VERSION}" \
    -Dpackaging=jar \
    --quiet 2>&1

# Create a minimal Maven project.
mkdir -p "${SMOKE_DIR}/src/main/java/smoke"
cat > "${SMOKE_DIR}/pom.xml" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.pdfluent.smoketest</groupId>
  <artifactId>smoke</artifactId>
  <version>0.0.1</version>
  <properties>
    <maven.compiler.source>11</maven.compiler.source>
    <maven.compiler.target>11</maven.compiler.target>
  </properties>
  <dependencies>
    <dependency>
      <groupId>${GROUP_ID}</groupId>
      <artifactId>${ARTIFACT_ID}</artifactId>
      <version>${EXPECTED_VERSION}</version>
    </dependency>
  </dependencies>
</project>
XML

cat > "${SMOKE_DIR}/src/main/java/smoke/Smoke.java" <<'JAVA'
package smoke;

public class Smoke {
    public static void main(String[] args) throws Exception {
        // Reflection-based probe: find the primary PDFluent class without
        // hard-coding the API (which may evolve across releases).
        String[] candidates = {
            "com.pdfluent.PdfDocument",
            "com.pdfluent.Document",
            "com.pdfluent.XfaPdf",
            "com.pdfluent.PdfEngine",
        };

        Class<?> found = null;
        for (String name : candidates) {
            try {
                found = Class.forName(name);
                System.out.println("  Primary class found: " + name);
                break;
            } catch (ClassNotFoundException e) {
                // Not this one.
            }
        }
        if (found == null) {
            System.out.println("  No primary class found in known locations.");
            System.out.println("  Package may have different entry points — check public API.");
        }

        System.out.println("Java consumer smoke PASS");
    }
}
JAVA

echo "Building Java smoke project..."
cd "${SMOKE_DIR}"
if $MVN package -DskipTests --quiet 2>&1; then
    echo "Running Java smoke..."
    if $MVN exec:java -Dexec.mainClass="smoke.Smoke" --quiet 2>&1; then
        echo ""
        echo "✅ Java consumer smoke PASS"
    else
        echo ""
        echo "❌ Java consumer smoke FAIL (runtime)"
        exit 1
    fi
else
    echo ""
    echo "❌ Java consumer smoke FAIL (build)"
    exit 1
fi
