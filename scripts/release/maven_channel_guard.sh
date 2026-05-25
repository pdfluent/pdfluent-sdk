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
# No network, no build, no publish. Pure static invariant check.
set -uo pipefail
cd "$(dirname "$0")/../.."

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
  grep -q 'LicenseRef-PDFluent-Commercial' "$CANON" && ok "canonical license=commercial" \
    || fail "canonical license must be LicenseRef-PDFluent-Commercial ($CANON)"
  grep -qE '<version>1\.0\.0-beta\.[0-9]+</version>' "$CANON" && ok "canonical version on RC line" \
    || fail "canonical version must be 1.0.0-beta.N ($CANON)"
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

echo "----------------------------------------"
if [ "$FAILS" -eq 0 ]; then echo "MAVEN_CHANNEL_GUARD: PASS"; exit 0; else echo "MAVEN_CHANNEL_GUARD: FAIL ($FAILS)"; exit 1; fi
