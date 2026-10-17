#!/usr/bin/env bash
# maven_channel_guard.sh — fail-closed guard for the PDFluent Maven channel.
#
# Enforces the canonical/deprecated split established by
# docs/reports/pdfluent_maven_channel_pom_reconciliation.md:
#
#   CANONICAL  bindings/java/pom.xml   -> com.pdfluent:pdfluent (commercial)
#   DEPRECATED crates/pdf-java/pom.xml -> com.pdfluent:xfa-pdf  (MIT, NON-PUBLISHABLE)
#
# It blocks (non-zero exit) if:
#   - the canonical pom drifts from com.pdfluent:pdfluent / commercial license;
#   - the deprecated xfa-pdf pom regains any publish capability
#     (distributionManagement, nexus-staging, or a non-skipped deploy);
#   - any CI/publish script wires `mvn deploy` against the deprecated path.
#
# No network, no build, no publish. Pure static invariant check, plus — when a
# built jar is present — the artefact gate over its contents (m30 fix 2: the
# guard used to read the POM and never open the jar, so the 1.0.0 jar with no
# natives at all passed).
#
#   maven_channel_guard.sh [--require-jar]
#
# Without --require-jar a missing jar is a warning (dev lane: nothing built
# yet). With it — the release lane, runbook §3.6 — a missing jar is a failure,
# and so is a jar that misses a /native/<platform>/ entry.
set -uo pipefail
cd "$(dirname "$0")/../.."

REQUIRE_JAR=0
for arg in "$@"; do
  case "$arg" in
    --require-jar) REQUIRE_JAR=1 ;;
    *) echo "MAVEN-GUARD-FAIL: unknown flag $arg"; exit 1 ;;
  esac
done

CANON="bindings/java/pom.xml"
LEGACY="crates/pdf-java/pom.xml"
FAILS=0
fail() { echo "MAVEN-GUARD-FAIL: $*"; FAILS=$((FAILS+1)); }
ok()   { echo "  ok: $*"; }

echo "== maven channel guard =="

# --- 1. canonical pom identity -------------------------------------------
if [ ! -f "$CANON" ]; then
  fail "canonical pom missing: $CANON"
else
  grep -q '<artifactId>pdfluent</artifactId>' "$CANON" && ok "canonical artifactId=pdfluent" \
    || fail "canonical artifactId must be 'pdfluent' ($CANON)"
  grep -q '<groupId>com.pdfluent</groupId>' "$CANON" && ok "canonical groupId=com.pdfluent" \
    || fail "canonical groupId must be 'com.pdfluent' ($CANON)"
  # LC9 (#221): the Maven artefact carries the dual licence, not commercial only.
  # The old check grepped for the commercial ref alone, which still matches the
  # dual string as a substring -- so it would have passed while reporting
  # "license=commercial" about an artefact that is no longer commercial-only.
  # Whole <name> element, not a substring. `grep -q` alone accepted
  # "... OR LicenseRef-PDFluent-Commercial OR MIT" -- an extra grant walking
  # through a gate whose whole job is to refuse extra grants. The previous
  # version of this check had the same flaw in the other direction: it matched
  # the commercial ref inside the dual string and reported "license=commercial"
  # about an artefact that was not.
  grep -qx '[[:space:]]*<name>AGPL-3.0-only OR LicenseRef-PDFluent-Commercial</name>[[:space:]]*' "$CANON" \
    && ok "canonical license=AGPL or commercial (exact)" \
    || fail "canonical <name> must be exactly 'AGPL-3.0-only OR LicenseRef-PDFluent-Commercial' ($CANON)"
  # THE VERSION IS COMPARED, NOT RECOGNISED (#358).
  #
  # This used to be `grep -qE '<version>1\.0\.0-beta\.[0-9]+</version>'`, and it
  # was wrong twice over. It named one release, so it went stale the day the
  # channel became 1.0.0 and refused a pom that
  # every_registry_names_the_same_release.py had just approved -- with section
  # 3.6 of the registry runbook running this guard immediately before
  # `mvn deploy`. And it matched ANY <version> element in the file, so a plugin
  # or a dependency on a beta number satisfied it while the project's own
  # coordinates said something else.
  #
  # Both halves are read by the functions the six-manifest guard uses, so the
  # two cannot disagree about what a release is, nor about which <version> in a
  # pom is the project's own.
  version_pair="$(python3 scripts/release/maven_canonical_version_pair.py "$CANON")" \
    || version_pair=""
  if [ -z "$version_pair" ]; then
    fail "could not read the release version and the pom version ($CANON)"
  else
    release="${version_pair%% *}"; pom_names="${version_pair##* }"
    if [ "$release" = "$pom_names" ]; then
      ok "canonical version is the release ($release)"
    else
      fail "canonical version must be the release $release, not $pom_names ($CANON)"
    fi
  fi
fi

# --- 2. deprecated pom must stay non-publishable -------------------------
if [ ! -f "$LEGACY" ]; then
  ok "legacy pom absent (fully removed) — no publish risk"
else
  # Must NOT carry publish plumbing.
  grep -q '<distributionManagement>' "$LEGACY" \
    && fail "deprecated $LEGACY must NOT have <distributionManagement>" \
    || ok "deprecated pom has no distributionManagement"
  grep -q 'nexus-staging-maven-plugin' "$LEGACY" \
    && fail "deprecated $LEGACY must NOT use nexus-staging-maven-plugin" \
    || ok "deprecated pom has no nexus-staging plugin"
  # Must explicitly skip deploy.
  if grep -q 'maven-deploy-plugin' "$LEGACY" && \
     awk '/maven-deploy-plugin/{f=1} f&&/<skip>true<\/skip>/{print "found"; exit}' "$LEGACY" | grep -q found; then
    ok "deprecated pom hard-disables deploy (skip=true)"
  else
    fail "deprecated $LEGACY must set maven-deploy-plugin <skip>true</skip>"
  fi
fi

# --- 3. no CI/publish script deploys the deprecated path -----------------
if grep -REn 'crates/pdf-java[^|]*mvn[[:space:]]+deploy|cd[[:space:]]+crates/pdf-java[[:space:]]*&&[[:space:]]*mvn[[:space:]]+deploy' \
   .gitlab-ci.yml scripts/ 2>/dev/null \
   | grep -v 'scripts/release/maven_channel_guard.sh' | grep -q .; then
  fail "a CI/publish script still runs 'mvn deploy' in crates/pdf-java (deprecated channel)"
else
  ok "no script deploys the deprecated crates/pdf-java path"
fi

# --- 4. the built jar covers every platform (m30 fix 2) -----------------
# The POM checks above say what SHOULD ship. This opens what DID ship: the
# main jar in bindings/java/target must carry /native/<platform>/ for all
# seven platforms. Checked by the shared artefact gate, not by a second list
# here, so the matrix has exactly one definition per channel.
jar="$(ls -t bindings/java/target/*.jar 2>/dev/null \
  | grep -v -e '-sources' -e '-javadoc' | head -n 1 || true)"
if [ -z "$jar" ]; then
  if [ "$REQUIRE_JAR" -eq 1 ]; then
    fail "no built jar in bindings/java/target (release lane: run mvn package first)"
  else
    echo "  warn: no built jar in bindings/java/target — contents unchecked (dev lane)"
  fi
elif python3 scripts/release/artifacts_cover_every_platform.py --jar "$jar"; then
  ok "jar covers every platform ($jar)"
else
  fail "jar misses a platform (see ARTIFACT-GATE above): $jar"
fi

echo "----------------------------------------"
if [ "$FAILS" -eq 0 ]; then echo "MAVEN_CHANNEL_GUARD: PASS"; exit 0; else echo "MAVEN_CHANNEL_GUARD: FAIL ($FAILS)"; exit 1; fi
