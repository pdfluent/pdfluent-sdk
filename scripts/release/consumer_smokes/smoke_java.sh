#!/usr/bin/env bash
# smoke_java.sh — Consumer smoke test for the PDFluent Java / Maven package.
#
# Creates a minimal Maven project, installs the .jar into the local Maven repo
# (does not require Maven Central publish), and extracts a fixture PDF to
# markdown through `com.pdfluent.PdfluentDocument` — which loads the JNI
# native in its static initializer. The old smoke did `Class.forName(...)`,
# which never touches the native library, printed PASS even when NO class was
# found, and exit-0'd ("skipped") when mvn or the jar was missing: the 1.0.0
# jar with no natives at all passed it.
#
# Lanes: in a release lane (PDFLUENT_RELEASE_LANE=1, set by
# post_publish_smoke_runner.sh) every skip is a failure. In a development lane
# a skip is green ONLY as `SKIPPED (not a pass)` — never as PASS.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_java.sh [--artifact PATH] [--jar PATH]
#       [--version VERSION] [--platform KEY] [--fixture PATH] [--sentence TEXT]
#
# Options:
#   --artifact PATH  Path to the .jar under test (same as --jar; this is what
#                    post_publish_smoke_runner.sh passes, and the smoke tests
#                    EXACTLY this file — never a neighbouring jar)
#   --jar PATH       Same as --artifact (default: auto-detect under
#                    bindings/java — dev lane only, see below)
#   --version VER    Expected artifact version (default: read from pom.xml)
#   --platform KEY   Must name this host (e.g. linux-arm64 on that host); the
#                    smoke runs where it is, against the given jar. Aliases
#                    macos-/darwin- to osx- and x86_64/aarch64 to x64/arm64.
#   --cross-check    Run even when --platform names another host. The explicit
#                    override: without it a mismatch is refused (exit 2).
#   --fixture PATH   Fixture PDF to extract (default: fixtures/structured.pdf)
#   --sentence TEXT  Known sentence the markdown must contain (default: Safety rules)
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.
#
# Lanes: in a release lane (PDFLUENT_RELEASE_LANE=1, set by
# post_publish_smoke_runner.sh) --artifact is required — auto-detect could
# pick any jar in the tree, and the smoke must vouch for the artefact about
# to ship.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_java]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
JAR_PATH=""
JAR_GIVEN=0
EXPECTED_VERSION=""
FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"
SENTENCE="Safety rules"
GROUP_ID="com.pdfluent"
ARTIFACT_ID="pdfluent"
STRICT="${PDFLUENT_RELEASE_LANE:-0}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact|--jar) JAR_PATH="$2"; JAR_GIVEN=1; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
        --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
        --fixture) FIXTURE="$2"; shift 2 ;;
        --sentence) SENTENCE="$2"; shift 2 ;;
        *) smoke_unknown_option "$1" ;;
    esac
done

smoke_check_platform

echo "=== PDFluent Java consumer smoke test ==="
[[ -n "$SMOKE_PLATFORM" ]] && echo "Platform: ${SMOKE_PLATFORM}"

skip_or_fail() { # $1 = reason
    if [[ "$STRICT" == "1" ]]; then
        echo "❌ $1 (release lane: a skip is a failure)"
        exit 1
    fi
    echo "SKIPPED (not a pass): $1"
    exit 0
}

# A given artefact must exist, naming the path (exit 2: argv /
# artefact-locate error, like smoke_node.sh). Auto-detect is the dev lane's
# convenience; the release lane names its jar.
if [[ "$JAR_GIVEN" == "1" ]]; then
    if [[ ! -f "$JAR_PATH" ]]; then
        echo "❌ no such jar: ${JAR_PATH}" >&2
        exit 2
    fi
    case "$JAR_PATH" in
        *.jar) ;;
        *) echo "❌ not a jar: ${JAR_PATH}" >&2; exit 2 ;;
    esac
elif [[ "$STRICT" == "1" ]]; then
    echo "❌ release lane needs --artifact: auto-detect could vouch for any jar in the tree" >&2
    exit 2
else
    # Auto-detect jar. `mapfile` is bash 4+; portable while-read loop for
    # macOS bash 3.2 (matches the smoke_dotnet.sh and sbom-generate.sh shape).
    candidates=()
    while IFS= read -r line; do candidates+=("$line"); done < <(find "${REPO_ROOT}/bindings/java" -name "*.jar" \
        -not -name "*javadoc*" -not -name "*sources*" 2>/dev/null | sort -r)
    if [[ ${#candidates[@]} -eq 0 ]]; then
        skip_or_fail "no .jar under bindings/java (build first: cd bindings/java && mvn package)"
    fi
    JAR_PATH="${candidates[0]}"
fi

# The toolchain is checked after the arguments: a bad --artifact or
# --platform is the caller's error (exit 2 above), not a missing tool.
if ! command -v mvn &>/dev/null && ! command -v mvnw &>/dev/null; then
    skip_or_fail "mvn not found in PATH"
fi

MVN=$(command -v mvn 2>/dev/null || echo "./mvnw")

if [[ ! -f "$JAR_PATH" ]]; then
    echo "❌ .jar not found: ${JAR_PATH}"
    exit 1
fi

if [[ ! -f "$FIXTURE" ]]; then
    echo "❌ Fixture not found: ${FIXTURE}"
    exit 1
fi

echo "Jar: ${JAR_PATH}"
echo "Fixture: ${FIXTURE}"

# Resolve version from pom.xml if not provided.
if [[ -z "$EXPECTED_VERSION" ]]; then
    EXPECTED_VERSION=$(grep -oPm1 '(?<=<version>)[^<]+' "${REPO_ROOT}/bindings/java/pom.xml" 2>/dev/null | head -1 || echo "1.0.0-beta.8")
elif [[ "$(basename "$JAR_PATH")" != *"$EXPECTED_VERSION"* ]]; then
    # An explicit --version that the jar name does not carry means the smoke
    # would install one artefact and vouch for another.
    echo "❌ jar $(basename "$JAR_PATH") does not match --version ${EXPECTED_VERSION}" >&2
    exit 2
fi
echo "Version: ${EXPECTED_VERSION}"
echo ""

# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
unset PDFLUENT_LICENSE_KEY

SMOKE_DIR=$(mktemp -d -t pdfluent_java_smoke_XXXXXX)
trap 'rm -rf "${SMOKE_DIR}"' EXIT

# Install the local jar into the local Maven repo.
echo "Installing jar into local Maven repo..."
# -Dfile= reaches the JVM unconverted by Git Bash: native spelling on Windows.
$MVN install:install-file \
    -Dfile="$(smoke_native_path "${JAR_PATH}")" \
    -DgroupId="${GROUP_ID}" \
    -DartifactId="${ARTIFACT_ID}" \
    -Dversion="${EXPECTED_VERSION}" \
    -Dpackaging=jar \
    --quiet 2>&1

# Create a minimal Maven project.
mkdir -p "${SMOKE_DIR}/src/main/java/smoke" "${SMOKE_DIR}/src/main/resources"
cp "$FIXTURE" "${SMOKE_DIR}/src/main/resources/fixture.pdf"
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

# No reflection, no fallback list: the smoke names the real entry point, opens
# a real PDF, and asserts a known sentence. A jar without natives fails here
# (UnsatisfiedLink from NativeLoader), and a jar without the class fails at
# compile time — both red, both loud.
cat > "${SMOKE_DIR}/src/main/java/smoke/Smoke.java" <<JAVA
package smoke;

import com.pdfluent.PdfluentDocument;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

public class Smoke {
    public static void main(String[] args) throws Exception {
        String sentence = args.length > 0 ? args[0] : "Safety rules";
        // Paths.get has a String and a URI overload, but no overload takes the
        // conditional of both: pick the Path in each branch.
        Path fixture = args.length > 1
            ? Paths.get(args[1])
            : Paths.get(Smoke.class.getResource("/fixture.pdf").toURI());
        if (!Files.exists(fixture)) {
            System.out.println("FAIL: fixture missing: " + fixture);
            System.exit(1);
        }
        try (PdfluentDocument doc = PdfluentDocument.open(fixture)) {
            String markdown = doc.extractMarkdown();
            if (!markdown.contains(sentence)) {
                System.out.println("FAIL: " + sentence + " not in output:");
                System.out.println(markdown.substring(0, Math.min(500, markdown.length())));
                System.exit(1);
            }
            System.out.println("  extractMarkdown: OK ("
                + markdown.length() + " chars, '" + sentence + "' found)");
            System.out.println("  first line: " + markdown.split("\n", 2)[0]);
        }
        System.out.println("Java consumer smoke PASS");
    }
}
JAVA

echo "Building Java smoke project..."
cd "${SMOKE_DIR}"
if $MVN package -DskipTests --quiet 2>&1; then
    echo "Running Java smoke..."
    if $MVN -q exec:java -Dexec.mainClass="smoke.Smoke" \
        -Dexec.args="'${SENTENCE}'" 2>&1; then
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
