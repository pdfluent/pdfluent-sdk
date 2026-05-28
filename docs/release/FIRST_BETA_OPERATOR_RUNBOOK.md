# PDFluent — First Staged Beta Operator Runbook

**Status:** mandatory · **Owner:** release captain · **Started:** 2026-05-28 (Tier-3 follow-on).

End-to-end runbook for the **first** staged PDFluent beta publish across
every channel listed in `RELEASE_TRAIN_MATRIX.md`. The runbook is a
checkbox list — every checked-off step has a definite artefact (a
committed audit report, a ledger entry, a smoke log, a publisher
acknowledgement). When all boxes are checked, the beta is published,
verified, ledger-recorded, and rollback-ready.

Use this runbook **only** for the first beta. Subsequent betas have less
one-time setup; for those use `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`
which assumes credentials are already provisioned.

---

## 0. Pre-conditions (must all be true before starting)

- [x] **MR !5 (Tier-1)** merged into `master` 2026-05-28 (merge commit
      `1c0814d04`). Verified via API.
- [ ] **MR !6 (Tier-2/Tier-3)** merged into `master`. Currently OPEN +
      mergeable; CI pipeline `2559947200` sanity stage 3/3 PASS. Awaits
      owner click. (Until this is merged, the runbook lives only on the
      `release-hardening/tier2` branch — operator must check out that
      branch to use the new scripts referenced below.)
- [ ] `git status --porcelain` is empty on the publish branch.
- [ ] Operator has read `PUBLISH_PROTOCOL.md`, `RELEASE_TRAIN_MATRIX.md`,
      and `ROLLBACK_PROCEDURE.md` in their current committed form.
- [ ] **§0.5 preflight commands below all PASS** for the channels the
      operator intends to publish in this session.

## 0.5 Pre-flight credential check (no-secret variant)

Run these before *any* publish session. Each command verifies presence +
basic shape of the credential the channel needs, without printing the
secret value. A FAIL means the channel cannot publish today; consult §3
for what to provision and where.

```bash
# crates.io — token shape only, no value
test -f ~/.cargo/credentials.toml && \
  python3 -c "import tomllib; t=tomllib.load(open('$HOME/.cargo/credentials.toml','rb'))['registry']['token']; assert len(t)>=30, 'crates.io token suspiciously short'; print('crates.io: OK')" || \
  echo 'crates.io: MISSING'

# npm @pdfluent — verify authenticated AND owner of @pdfluent org
npm whoami --registry=https://registry.npmjs.org/ >/dev/null 2>&1 && \
  npm org ls @pdfluent 2>/dev/null | grep -E "$(npm whoami) - owner" >/dev/null && \
  echo 'npm @pdfluent: OK (owner-verified)' || \
  echo 'npm @pdfluent: MISSING or not owner of @pdfluent'

# PyPI — token shape (must start with `pypi-`)
val=$(security find-generic-password -s pypi-token -w 2>/dev/null)
[[ "$val" =~ ^pypi- ]] && echo 'PyPI: OK' || echo 'PyPI: MISSING or wrong format'
unset val

# NuGet — API key shape (must start with `oy2` for v3)
val=$(security find-generic-password -s nuget-api-key -w 2>/dev/null)
[[ "$val" =~ ^oy2 ]] && echo 'NuGet: OK' || echo 'NuGet: MISSING or wrong format'
unset val

# Maven Central — server config + GPG secret-key, each reported independently.
# pinentry-mac is OPTIONAL: only needed for non-interactive GPG passphrase
# entry (CI-only requirement). Operator-on-Mac runs interactively so a TTY
# prompt is fine — preflight reports it but does not block.
test -f ~/.m2/settings.xml && \
  python3 -c "import xml.etree.ElementTree as ET; r=ET.parse('$HOME/.m2/settings.xml').getroot(); assert any((s.find('id') or s.find('{http://maven.apache.org/SETTINGS/1.0.0}id')).text=='central' for s in r.iter() if s.tag.endswith('server')), 'central server not in settings.xml'; print('maven settings: OK')" \
  || echo 'maven settings: MISSING'
gpg --list-secret-keys hello@pdfluent.com >/dev/null 2>&1 && echo 'maven GPG: OK' || echo 'maven GPG: MISSING'
( test -x /usr/local/opt/pinentry-mac/bin/pinentry-mac \
    || test -x /opt/homebrew/opt/pinentry-mac/bin/pinentry-mac ) \
  && echo 'pinentry-mac: OK (CI non-interactive ready)' \
  || echo 'pinentry-mac: not installed (operator-on-Mac TTY prompt fine; install via `brew install pinentry-mac` for CI use)'

# Apple Developer ID Application — required for macOS sign + notarize
security find-identity -v -p codesigning | grep -E 'Developer ID Application' >/dev/null && \
  echo 'macOS Developer ID: OK' || echo 'macOS Developer ID: MISSING'

# Microsoft Trusted Signing (Azure) — only sensible if AZ creds set
[ -n "${AZ_TENANT_ID:-}" ] && [ -n "${AZ_CLIENT_ID:-}" ] && [ -n "${AZ_CLIENT_SECRET:-}" ] && \
  echo 'Windows Trusted Signing: env present (verify in §3.7)' || \
  echo 'Windows Trusted Signing: env not set (use USB EV fallback if available)'

# GitLab API token — for `glab release create` and Releases upload
security find-internet-password -s gitlab.com -a claude-pdfluent-api -w >/dev/null 2>&1 && \
  echo 'GitLab API: OK' || echo 'GitLab API: MISSING'
```

**Current verified state on the operator Mac (2026-05-28):**

| channel | preflight result | go/no-go today |
|---|---|---|
| crates.io | `OK` | 🟢 go |
| npm `@pdfluent` | `OK (owner-verified)` | 🟢 go |
| PyPI | `OK` | 🟢 go |
| NuGet | `OK` | 🟢 go |
| Maven Central | `OK` (settings + GPG; pinentry-mac not installed but only needed for CI — TTY passphrase prompt is fine on operator-on-Mac) | 🟢 go |
| macOS binary signing | `MISSING` | 🔴 no — needs Apple Developer ID Application + notarytool API key (§3.6) |
| Windows binary signing | `MISSING` | 🔴 no — needs Trusted Signing or USB EV (§3.7) |
| GitLab Releases upload | `OK` | 🟢 go (for unsigned/Linux-GPG tarballs) |

## 1. Per-channel operator checklist

The publish order is **strict**, per
`RELEASE_TRAIN_MATRIX.md §"Topological publish order"`. Each row below
is one publish target. Mark every checkbox as it completes; do NOT skip
ahead.

### 1.1 crates.io — topological chain (channel #1)

The 32 publishable Rust crates publish in topological order. The first
beta is gated on the **4-crate chain catch-up**:
`formcalc-interpreter @ 1.0.0-beta.8` → `pdf-compliance @ 1.0.0-beta.8`
→ `pdf-xfa @ 1.0.0-beta.8` → `pdfluent @ 1.0.0-beta.8`. Current
crates.io state has only beta.5/beta.7 of these; the chain is the **R2-1
publish-train work** (`benchmarks/runs/prepublish_audits/2026-05-28_rc1_aa9ba515b_bite-c_note.md`).

For each crate in topological order:

```bash
# 0. Move into a clean checkout of the merged master head.
git switch master
git pull --ff-only

# 1. Prepublish audit.
bash scripts/release/prepublish_crate_audit.sh <crate>
# Produces benchmarks/runs/prepublish_audits/<crate>-<v>.md — review + commit.

# 2. SBOM check passes (this is also enforced by CI, but run locally for
#    operator confidence).
bash scripts/release/sbom-generate.sh --check

# 3. cargo package (the audit script already did this; redo for clean
#    .crate file path).
cargo package --allow-dirty=false -p <crate>
# Locate the .crate at target/package/<crate>-<v>.crate.

# 4. Publish.
# 🚦 OPERATOR APPROVAL ONLY — irreversible: crates.io is append-only, no
#    re-upload of the same version after this returns 0. Verify §0.5 says
#    `crates.io: OK` before running. Token from ~/.cargo/credentials.toml.
cargo publish -p <crate>

# 5. Append the SHA ledger entry IMMEDIATELY after the publish step
#    returns 0 (per R6-1).
python3 scripts/release/ledger_add_entry.py \
  --channel crates_io \
  --package <crate> \
  --version 1.0.0-beta.8 \
  --artifact target/package/<crate>-1.0.0-beta.8.crate \
  --audit-report benchmarks/runs/prepublish_audits/<crate>-1.0.0-beta.8.md \
  --registry-url https://static.crates.io/crates/<crate>/<crate>-1.0.0-beta.8.crate

# 6. Wait 60s for registry propagation, then post-publish verify.
sleep 60
python3 scripts/release/ledger_verify.py \
  --channel crates_io \
  --package <crate> \
  --version 1.0.0-beta.8 \
  --write \
  --verify-report benchmarks/runs/post_publish_verify/<crate>-1.0.0-beta.8.md

# 7. Smoke (consumer-side).
bash docs/release/consumer_smokes/smoke_rust.sh --crate <crate> --version 1.0.0-beta.8

# 8. git add the ledger file + audit report + verify report + commit.
git add docs/release/sha_ledger/crates_io.json
git add benchmarks/runs/prepublish_audits/<crate>-1.0.0-beta.8.md
git add benchmarks/runs/post_publish_verify/<crate>-1.0.0-beta.8.md
git commit -m "release: publish <crate> 1.0.0-beta.8 to crates.io"
git push origin master
```

Repeat for every crate in `RELEASE_TRAIN_MATRIX.md §"Topological publish
order"` §1..§7.

**Sub-checklist (mark per crate):**

- [ ] `formcalc-interpreter @ 1.0.0-beta.8` (currently latest crates.io is beta.5)
- [ ] `pdf-compliance @ 1.0.0-beta.8`     (currently latest crates.io is beta.7)
- [ ] `pdf-xfa @ 1.0.0-beta.8`            (depends on formcalc-interpreter)
- [ ] `pdfluent @ 1.0.0-beta.8`           (umbrella; last in the chain)
- [ ] (the other 28 publishable crates, per §1..§6 of the topological order)

### 1.2 npm — WASM SDK (`@pdfluent/sdk-wasm`, channel #3)

```bash
# 1. Build the WASM package.
( cd crates/xfa-wasm && wasm-pack build --release --target web )
# Output: crates/xfa-wasm/pkg/

# 2. Apply the publish-shape transform (creates the public package.json).
bash scripts/release/transform-wasm-pkg.sh

# 3. Dry-run + audit gate (R1-3).
bash scripts/release/wasm_dry_run.sh
# Must exit 0. Save the report under benchmarks/runs/ga_hardening_plan/release/r1/.

# 4. npm pack (produces the .tgz that npm publish will upload).
( cd crates/xfa-wasm/pkg && npm pack )
# Output: pdfluent-sdk-wasm-1.0.0-beta.8.tgz

# 5. Publish.
# 🚦 OPERATOR APPROVAL ONLY — irreversible: `npm unpublish` after 72h is
#    blocked; the version name is forever-taken on `@pdfluent/sdk-wasm`.
#    Verify §0.5 says `npm @pdfluent: OK (owner-verified)` before running.
( cd crates/xfa-wasm/pkg && npm publish --access public )

# 6. SHA ledger.
python3 scripts/release/ledger_add_entry.py \
  --channel wasm \
  --package @pdfluent/sdk-wasm \
  --version 1.0.0-beta.8 \
  --artifact crates/xfa-wasm/pkg/pdfluent-sdk-wasm-1.0.0-beta.8.tgz \
  --audit-report benchmarks/runs/prepublish_audits/sdk-wasm-1.0.0-beta.8.md \
  --registry-url https://registry.npmjs.org/@pdfluent/sdk-wasm/-/sdk-wasm-1.0.0-beta.8.tgz

# 7. Post-publish verify + smoke.
sleep 30
python3 scripts/release/ledger_verify.py --channel wasm --package @pdfluent/sdk-wasm --version 1.0.0-beta.8 --write
bash docs/release/consumer_smokes/smoke_wasm.sh --pkg-dir crates/xfa-wasm/pkg
```

- [ ] `@pdfluent/sdk-wasm @ 1.0.0-beta.8` published + verified + smoked.

### 1.3 npm — NAPI binding (`@pdfluent/node`, channel #2)

```bash
# 1. Local cargo check (Tier-1 CI does this too).
cargo check -p pdf-node

# 2. (Operator builds NAPI native binaries per pdf-node's release process —
#    out of scope for THIS runbook; pdf-node currently ships as JS-only
#    glue + cargo build at install time, OR uses napi-rs prebuilt
#    binaries. Confirm which model is in place before proceeding.)

# 3. npm pack.
( cd crates/pdf-node && npm pack )
# Output: pdfluent-node-1.0.0-beta.8.tgz (per Tier-1 CI logs: 15 KB).

# 4. Audit.
python3 scripts/release/audit_package_tree.py \
  --tree crates/pdf-node --out crates/pdf-node/target/audit \
  --package-name pdfluent-node --package-version 1.0.0-beta.8 \
  --channel npm

# 5. Publish.
# 🚦 OPERATOR APPROVAL ONLY — same npm constraints as §1.2 (72h unpublish
#    window then forever-taken). Verify §0.5 says `npm @pdfluent: OK`.
( cd crates/pdf-node && npm publish --access public )

# 6. Ledger + verify + smoke (same shape as §1.2).
python3 scripts/release/ledger_add_entry.py --channel npm \
  --package @pdfluent/node --version 1.0.0-beta.8 \
  --artifact crates/pdf-node/pdfluent-node-1.0.0-beta.8.tgz \
  --audit-report benchmarks/runs/prepublish_audits/node-1.0.0-beta.8.md \
  --registry-url https://registry.npmjs.org/@pdfluent/node/-/node-1.0.0-beta.8.tgz
sleep 30
python3 scripts/release/ledger_verify.py --channel npm --package @pdfluent/node --version 1.0.0-beta.8 --write
bash docs/release/consumer_smokes/smoke_node.sh --artifact crates/pdf-node/pdfluent-node-1.0.0-beta.8.tgz
```

- [ ] `@pdfluent/node @ 1.0.0-beta.8` published + verified + smoked.

### 1.4 PyPI (`pdfluent`, channel #4)

```bash
# 1. Build wheel via maturin.
( cd crates/pdf-python && maturin build --release )
# Output: crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*-*.whl

# 2. Scrub the wheel for private paths / corpus / etc.
python3 scripts/release/scrub-wheel.py crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*.whl

# 3. Twine check (PyPI uploader's pre-publish gate).
python3 -m twine check crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*.whl

# 4. Publish to TestPyPI FIRST (staged smoke), then to PyPI proper.
# 🚦 OPERATOR APPROVAL ONLY (staging) — TestPyPI is staging but still
#    burns the version name on test.pypi.org for `pdfluent`. Reversibility:
#    can be yanked via test.pypi.org Manage UI; no hard delete.
#    Token from keychain `pypi-token` via TWINE_PASSWORD.
TWINE_USERNAME=__token__ TWINE_PASSWORD="$(security find-generic-password -s pypi-token -w)" \
  python3 -m twine upload --repository testpypi crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*.whl
# Smoke from TestPyPI:
bash docs/release/consumer_smokes/smoke_python.sh --index-url https://test.pypi.org/simple/

# 5. Real publish (once TestPyPI smoke passes).
# 🚦 OPERATOR APPROVAL ONLY — irreversible: PyPI does not allow re-upload
#    of the same version after first upload, even after delete. The version
#    name `pdfluent==1.0.0b8` is forever-taken on pypi.org after this returns 0.
TWINE_USERNAME=__token__ TWINE_PASSWORD="$(security find-generic-password -s pypi-token -w)" \
  python3 -m twine upload crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*.whl

# 6. Ledger + verify + smoke.
python3 scripts/release/ledger_add_entry.py --channel pypi \
  --package pdfluent --version 1.0.0b8 \
  --artifact crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*.whl \
  --audit-report benchmarks/runs/prepublish_audits/pdfluent-py-1.0.0b8.md \
  --registry-url 'https://files.pythonhosted.org/packages/.../pdfluent-1.0.0b8-*-*.whl'
sleep 30
python3 scripts/release/ledger_verify.py --channel pypi --package pdfluent --version 1.0.0b8 --write
bash docs/release/consumer_smokes/smoke_python.sh
```

- [ ] `pdfluent @ 1.0.0b8` published to TestPyPI; smoked.
- [ ] `pdfluent @ 1.0.0b8` published to PyPI proper; verified + smoked.

### 1.5 Maven Central (`com.pdfluent:pdfluent`, channel #5)

```bash
# 1. Build the JAR (with sources + javadoc per Maven Central rules).
( cd bindings/java && mvn package -DskipTests )
# Output: bindings/java/target/pdfluent-1.0.0-beta.8.jar
#         + pdfluent-1.0.0-beta.8-sources.jar
#         + pdfluent-1.0.0-beta.8-javadoc.jar

# 2. Maven channel guard.
bash scripts/release/maven_channel_guard.sh

# 3. Deploy to Sonatype OSSRH staging (NOT release).
# 🚦 OPERATOR APPROVAL ONLY — deploys to Sonatype OSSRH **staging** (the
#    staged repo is reversible: `mvn nexus-staging:drop` before release).
#    The follow-up "release the staging repo" web-UI click in §1.5 step 5
#    IS irreversible (Maven Central is immutable post-release). Maven
#    credentials read from ~/.m2/settings.xml + GPG key from gpg-agent
#    (pinentry-mac handles the passphrase prompt non-interactively).
( cd bindings/java && mvn deploy -P release -DperformRelease=true )

# 4. SMOKE TEST from STAGING (drop-able if anything is wrong).
bash docs/release/consumer_smokes/smoke_java.sh --jar bindings/java/target/pdfluent-1.0.0-beta.8.jar

# 5. If smoke OK: release the staging repository (via Sonatype web UI;
#    one-click). If anything is wrong: drop the staging repo and DO NOT
#    release (Maven Central is immutable post-release).

# 6. Ledger + verify (after the staging release has propagated to Maven
#    Central, ~30 min to 4 hours).
python3 scripts/release/ledger_add_entry.py --channel maven \
  --package com.pdfluent:pdfluent --version 1.0.0-beta.8 \
  --artifact bindings/java/target/pdfluent-1.0.0-beta.8.jar \
  --audit-report benchmarks/runs/prepublish_audits/pdfluent-java-1.0.0-beta.8.md \
  --registry-url https://repo1.maven.org/maven2/com/pdfluent/pdfluent/1.0.0-beta.8/pdfluent-1.0.0-beta.8.jar
sleep 1800    # 30 min for Central sync
python3 scripts/release/ledger_verify.py --channel maven --package com.pdfluent:pdfluent --version 1.0.0-beta.8 --write
```

- [ ] `com.pdfluent:pdfluent @ 1.0.0-beta.8` deployed to staging.
- [ ] Smoke from staging PASS.
- [ ] Staging released to Maven Central.
- [ ] Ledger verified after Central sync.

### 1.6 NuGet (`PDFluent`, channel #6)

```bash
# 1. Stage native binaries.
bash scripts/release/stage_dotnet_natives.sh

# 2. Pack.
( cd bindings/dotnet/src/PDFluent && dotnet pack -c Release )
# Output: bindings/dotnet/src/PDFluent/bin/Release/PDFluent.1.0.0-beta.8.nupkg

# 3. Audit.
python3 scripts/release/audit_package_tree.py \
  --tree bindings/dotnet/src/PDFluent --out bindings/dotnet/src/PDFluent/target/audit \
  --package-name PDFluent --package-version 1.0.0-beta.8 \
  --channel maven   # NuGet uses similar shape; "maven" channel is closest

# 4. Push.
# 🚦 OPERATOR APPROVAL ONLY — NuGet allows `dotnet nuget delete` (unlist
#    only; the version stays installable by exact-version pin forever).
#    No hard delete. Reversibility: relist via the nuget.org UI.
#    API key read from keychain `nuget-api-key`; --api-key arg requires it.
dotnet nuget push bindings/dotnet/src/PDFluent/bin/Release/PDFluent.1.0.0-beta.8.nupkg \
  --source https://api.nuget.org/v3/index.json \
  --api-key "$(security find-generic-password -s nuget-api-key -w)"

# 5. Ledger + verify + smoke.
python3 scripts/release/ledger_add_entry.py --channel nuget \
  --package PDFluent --version 1.0.0-beta.8 \
  --artifact bindings/dotnet/src/PDFluent/bin/Release/PDFluent.1.0.0-beta.8.nupkg \
  --audit-report benchmarks/runs/prepublish_audits/PDFluent-1.0.0-beta.8.md \
  --registry-url https://www.nuget.org/api/v2/package/PDFluent/1.0.0-beta.8
sleep 60
python3 scripts/release/ledger_verify.py --channel nuget --package PDFluent --version 1.0.0-beta.8 --write
bash docs/release/consumer_smokes/smoke_dotnet.sh --nupkg bindings/dotnet/src/PDFluent/bin/Release/PDFluent.1.0.0-beta.8.nupkg
```

- [ ] `PDFluent @ 1.0.0-beta.8` published + verified + smoked.

### 1.7 C-ABI tarball (channel #7)

```bash
# 1. Build the tarball.
bash scripts/release/package_cabi.sh
# Output (per the script's own report): dist/pdf-capi-1.0.0-beta.8-<target>.tar.gz

# 2. Audit the unpacked tree.
python3 scripts/release/audit_package_tree.py \
  --tree dist/pdf-capi-1.0.0-beta.8 \
  --out dist/pdf-capi-1.0.0-beta.8/audit \
  --package-name pdf-capi --package-version 1.0.0-beta.8 \
  --channel binary

# 3. Upload to distribution storage (operator-specific — Hetzner Storage
#    Box, S3, GitLab Release artefact, etc.).
#    Record the canonical URL the artefact ends up at.

# 4. Ledger + verify.
python3 scripts/release/ledger_add_entry.py --channel binary \
  --package pdf-capi --version 1.0.0-beta.8 \
  --artifact dist/pdf-capi-1.0.0-beta.8-x86_64-unknown-linux-gnu.tar.gz \
  --audit-report benchmarks/runs/prepublish_audits/pdf-capi-1.0.0-beta.8.md \
  --registry-url '<the canonical URL from step 3>'
sleep 30
python3 scripts/release/ledger_verify.py --channel binary --package pdf-capi --version 1.0.0-beta.8 --write
```

- [ ] `pdf-capi @ 1.0.0-beta.8` (per host target) built + uploaded + verified.

### 1.8 CLI binary release (channel #8)

The Tier-1 + Tier-3 work already CI-builds two of the four targets. The
runbook is:

| target | how to obtain | sign |
|---|---|---|
| `x86_64-unknown-linux-musl` | Tier-1 CI `package:binary-release` (artefact `pdfluent-1.0.0-beta.8-x86_64-unknown-linux-musl.tar.gz`, 5.65 MB) | sign with GPG `.asc` per existing checklist |
| `x86_64-pc-windows-gnu` | Tier-1 CI `package:binary-release` (artefact `pdfluent-1.0.0-beta.8-x86_64-pc-windows-gnu.tar.gz`, 5.29 MB) | Microsoft Trusted Signing per `SIGNING_WINDOWS.md` (or USB EV fallback) |
| `x86_64-apple-darwin` | operator builds on a Mac with `cargo build --release --target x86_64-apple-darwin -p xfa-cli`; staged into `dist/x86_64-apple-darwin/` | Apple Dev ID + notarization per `SIGNING_MACOS.md` |
| `aarch64-apple-darwin` | operator builds on Apple Silicon Mac with default target; staged into `dist/aarch64-apple-darwin/` | Apple Dev ID + notarization per `SIGNING_MACOS.md` |

```bash
# For each target tarball, after signing:
python3 scripts/release/ledger_add_entry.py --channel binary \
  --package pdfluent --version 1.0.0-beta.8 \
  --artifact dist/pdfluent-1.0.0-beta.8-<target>.tar.gz \
  --audit-report benchmarks/runs/prepublish_audits/pdfluent-cli-<target>-1.0.0-beta.8.md \
  --registry-url '<the canonical Release attachment URL>'
sleep 30
python3 scripts/release/ledger_verify.py --channel binary --package pdfluent --version 1.0.0-beta.8 --write
bash docs/release/consumer_smokes/smoke_binary.sh --artifact dist/pdfluent-1.0.0-beta.8-<target>.tar.gz
```

GitLab Release attachment upload (the operator-only step):

```bash
# 🚦 OPERATOR APPROVAL ONLY — creates a Release tag `1.0.0-beta.8` on the
#    project (REVERSIBLE: `glab release delete 1.0.0-beta.8`). Note: the
#    project CI workflow rules treat `$CI_COMMIT_TAG` as a publish-trigger
#    — confirm no downstream CI publish job fires from this tag before
#    running, or the cascade may push to channels that are already
#    published earlier in this runbook.
glab release create "1.0.0-beta.8" --name "PDFluent 1.0.0-beta.8" \
  --notes-file release-notes.md \
  --ref "<the merged-master SHA>"
glab release upload "1.0.0-beta.8" \
  dist/pdfluent-1.0.0-beta.8-x86_64-unknown-linux-musl.tar.gz \
  dist/pdfluent-1.0.0-beta.8-x86_64-unknown-linux-musl.tar.gz.sha256 \
  dist/pdfluent-1.0.0-beta.8-x86_64-pc-windows-gnu.tar.gz \
  dist/pdfluent-1.0.0-beta.8-x86_64-pc-windows-gnu.tar.gz.sha256 \
  dist/pdfluent-1.0.0-beta.8-x86_64-apple-darwin.tar.gz \
  dist/pdfluent-1.0.0-beta.8-aarch64-apple-darwin.tar.gz
```

⚠️ **Note:** the Release tag `1.0.0-beta.8` is NOT a `v*` tag. The
existing CI workflow rules treat `$CI_COMMIT_TAG` as a publish-trigger;
re-confirm with the operator that the tag will NOT cascade-trigger
publish on already-published channels.

- [ ] musl tarball ledgered + smoked.
- [ ] windows-gnu tarball SIGNED, ledgered + smoked.
- [ ] x86_64-darwin tarball SIGNED + notarized, ledgered + smoked.
- [ ] aarch64-darwin tarball SIGNED + notarized, ledgered + smoked.
- [ ] GitLab Release attachments uploaded for all four targets.

### 1.9 GitLab Package Registry (channel #9)

Internal mirror. Useful for staging before public registries. Out of
scope for the *first public* beta unless the operator chooses to stage
there first.

## 2. Aggregate verification

After all channels above are checked off:

- [ ] Every channel's ledger file under `docs/release/sha_ledger/`
      contains a `status = verified` entry for `1.0.0-beta.8`.
- [ ] `scripts/release/post_publish_smoke_runner.sh --channel all` PASS
      against the locally-built artefacts (the same artefacts uploaded
      above; this is the all-in-one smoke confirmation).
- [ ] A `1.0.0-beta.8` Git tag is **NOT** yet pushed — the v\*-tag
      decision is a follow-on (separate operator approval).
- [ ] `CHANGELOG.md` already has the `1.0.0-beta.8` entry from Tier-1;
      verify it accurately reflects what shipped.
- [ ] Public announcement (release notes blog post, Discord, mailing
      list) drafted and queued for the post-publish-verify window.

## 3. Credentials checklist

Every secret listed below MUST be provisioned BEFORE the corresponding
channel publishes. Store every secret in the operator's macOS keychain
(local development) AND as a **masked + protected** GitLab CI/CD
variable (CI use). Never in the repo. Never in a screen-recorded
terminal.

> **Live state:** §0.5 above runs the per-credential preflight checks
> against the operator Mac and reports OK / MISSING. As of 2026-05-28 the
> verified state is: crates.io ✓, npm `@pdfluent` ✓ (owner-verified),
> PyPI ✓, NuGet ✓, Maven Central + GPG + pinentry-mac ✓, GitLab API ✓,
> macOS Developer ID ✗ (only Apple Development certs present), Windows
> signing ✗.

### 3.1 crates.io API token

| field | value |
|---|---|
| where to get it | https://crates.io → account → "API Tokens" → "New Token" |
| scope | `publish-new` and `publish-update` for the PDFluent crates |
| keychain | `security add-internet-password -s crates.io -a pdfluent-publish -w '<token>'` |
| GitLab CI/CD variable | `CARGO_REGISTRY_TOKEN` |
| protected / masked | masked + protected (publish jobs only run on tag/MR) |
| needed for | actual publish only (NOT for dry-run; `cargo publish --dry-run` does not require credentials) |

### 3.2 npm automation token (`@pdfluent` org)

| field | value |
|---|---|
| where to get it | https://www.npmjs.com → org `@pdfluent` → Settings → Tokens → "Generate Automation Token" |
| scope | `publish` on the `@pdfluent` scope |
| keychain | `security add-internet-password -s registry.npmjs.org -a '@pdfluent-publish' -w '<token>'` |
| GitLab CI/CD variable | `NPM_TOKEN` |
| protected / masked | masked + protected |
| needed for | actual publish; `npm pack --dry-run` does NOT require it |

### 3.3 PyPI API token

| field | value |
|---|---|
| where to get it | https://pypi.org → account → "API tokens" → "Add API token", scoped to project `pdfluent` |
| keychain | `security add-internet-password -s pypi.org -a __token__ -w '<token>'` |
| GitLab CI/CD variable | `TWINE_USERNAME=__token__` + `TWINE_PASSWORD=<token>` (twine's documented pattern) |
| protected / masked | both masked + protected |
| needed for | actual upload to TestPyPI and PyPI; `twine check` is local-only |

### 3.4 Maven Central — Sonatype OSSRH + GPG

| field | value |
|---|---|
| Sonatype OSSRH login | https://issues.sonatype.org → account creation + project ticket OR new-portal `central.sonatype.com` (post-2024 flow) |
| keychain (OSSRH) | `security add-internet-password -s s01.oss.sonatype.org -a '<sonatype-user>' -w '<password>'` |
| GitLab CI/CD variables | `SONATYPE_USERNAME`, `SONATYPE_PASSWORD` |
| GPG key | generate per https://central.sonatype.org/publish/requirements/gpg/ ; export public key to Ubuntu / MIT keyservers |
| GPG keychain | gpg-agent loads on demand; the passphrase is held in keychain via `pinentry-mac` |
| GitLab CI/CD variables (GPG) | `GPG_PRIVATE_KEY` (ASCII-armored), `GPG_PASSPHRASE` |
| protected / masked | all 4 masked + protected |
| needed for | `mvn deploy` + Sonatype staging release |

### 3.5 NuGet API key

| field | value |
|---|---|
| where to get it | https://www.nuget.org → account → "API Keys" → "Create" with scope = "Push" on package `PDFluent` |
| keychain | `security add-internet-password -s nuget.org -a pdfluent-publish -w '<key>'` |
| GitLab CI/CD variable | `NUGET_API_KEY` |
| protected / masked | masked + protected |
| needed for | `dotnet nuget push` |

### 3.6 macOS Developer ID + notarytool

Per `docs/release/SIGNING_MACOS.md` §2:

| field | value |
|---|---|
| Apple Developer Program enrolment | $99/yr; the publishing entity must have a verified D-U-N-S number for corporate (or self-employed) |
| Developer ID Application certificate | Xcode → Preferences → Accounts → Manage Certificates → Create. Verify with `security find-identity -v -p codesigning`. |
| notarytool API key | App Store Connect → Users and Access → Integrations → App Store Connect API. Key type = Developer. Download `.p8` once. |
| keychain (notarytool profile) | `xcrun notarytool store-credentials "pdfluent-notarytool" --key <p8> --key-id <KeyID> --issuer <IssuerID>` |
| GitLab CI/CD variables | `MACOS_CERT_P12_B64`, `MACOS_CERT_P12_PASSWORD`, `MACOS_NOTARYTOOL_PROFILE`. **All three masked + protected**. |
| needed for | macOS binary release only |

### 3.7 Windows code-signing

Per `docs/release/SIGNING_WINDOWS.md` §2.1 (preferred) or §2.3 (fallback):

| field | value |
|---|---|
| Microsoft Trusted Signing | https://learn.microsoft.com/en-us/azure/trusted-signing/ ; Azure subscription required; Identity Validation through a corporate D-U-N-S number; per-signature or per-certificate billing |
| Trusted Signing — Azure resource | `az signing-account create …` + Trusted Signing Profile + service-principal Code Signer role |
| GitLab CI/CD variables (Trusted Signing path) | `AZ_TENANT_ID`, `AZ_CLIENT_ID`, `AZ_CLIENT_SECRET`, `TS_ACCOUNT`, `TS_PROFILE`. **All five masked + protected**. |
| Fallback: USB-token EV cert | existing Sectigo/DigiCert/Globalsign cert; cert thumbprint + token PIN |
| keychain (PIN, if using fallback) | per-operator-machine; **CI-incompatible** (manual sign step required) |
| needed for | Windows binary release only |

### 3.8 GitLab Releases (artefact upload)

| field | value |
|---|---|
| GitLab API token | `claude-pdfluent-api` (already in keychain, scope = `api`, expires 2027-05-28) is sufficient for `glab release create` + `glab release upload`. |
| GitLab CI/CD variable | `GLAB_TOKEN` if using glab in CI; the `CI_JOB_TOKEN` is sufficient for `release: create` via the API |
| needed for | uploading the binary release tarballs to GitLab Releases |

## 4. Signing plan — first release manual path

The first beta does NOT need a fully-automated signing CI lane. The
minimum viable path is:

### 4.1 macOS first-release manual path

Operator-on-Mac, per `SIGNING_MACOS.md` §3:

1. Build the two macOS targets locally (`cargo build --release --target
   x86_64-apple-darwin -p xfa-cli` and the aarch64 equivalent).
2. Sign + notarize each per `SIGNING_MACOS.md` §3 (the `codesign` →
   `ditto` → `xcrun notarytool submit --wait` → `spctl --assess` flow).
3. Tar the signed bundles into `dist/pdfluent-1.0.0-beta.8-<target>.tar.gz`.
4. Stage to the workspace `dist/` for ledger + smoke.

Estimated wall-clock per target: 15–30 min (notarization is ~5–15 min).

### 4.2 Windows first-release manual path

There are two practically-viable paths for the FIRST release; the
operator chooses based on which is already provisioned.

**Path A — USB-token EV (if already owned).** Operator-on-Windows
machine (or operator-on-Mac with the token plugged into a USB-attached
Windows VM). Use `signtool sign /fd sha256 /tr <ts-url> /td sha256
/sha1 <thumbprint>` per `SIGNING_WINDOWS.md` §2.3. Tag the signed
binary `dist/pdfluent-1.0.0-beta.8-x86_64-pc-windows-gnu.tar.gz` after
re-tarring.

**Path B — Microsoft Trusted Signing (new).** Requires the Azure
provisioning steps in `SIGNING_WINDOWS.md` §2.1.1 (one-time, ~1 hour).
After provisioning the operator uses `dotnet sign code
azure-trusted-signing …` from any host with Azure CLI authenticated.

For the **first** beta, Path A is faster if the EV token is already
owned. Path B is the right answer for the second beta onward (and any
future operator who joins).

### 4.3 Later automation

Once Microsoft Trusted Signing is provisioned, the Windows-signing CI
job from `SIGNING_WINDOWS.md` §4 Pattern A can be wired into
`.gitlab-ci.yml` as a new manual job in the `package_manual` stage
gated on `$CI_COMMIT_TAG` + `merge_request_event`. The macOS-signing
job remains operator-on-Mac because GitLab has no macOS runner on the
PDFluent VPS.

## 5. Stop-the-line per channel

If any of the following triggers during the run, **stop immediately**,
do not progress to the next channel, and follow `ROLLBACK_PROCEDURE.md`:

| trigger | which channel | what to do |
|---|---|---|
| `prepublish_crate_audit.sh` exits non-zero | crates.io | inspect the per-crate audit; fix or escalate; do not `cargo publish` |
| `cargo publish` returns "version already exists" | crates.io | the chain is out of order; check the topological order matrix |
| `ledger_verify.py` reports DRIFT (sha mismatch) | any | §11 / §12 of `PUBLISH_PROTOCOL.md` apply; this is a critical post-publish defect |
| `npm publish` returns 403 | npm | the `@pdfluent` org token scope is wrong; do NOT retry without checking |
| `twine upload` returns 400 with "File already exists" | PyPI | PyPI does not allow re-upload of the same version; the version is now used forever |
| Sonatype Staging closure fails | Maven | drop the staging repo; rebuild; do NOT release |
| `spctl --assess` rejects notarized macOS binary | binary | re-notarize after fixing the rejection reason in the notarytool log |
| `signtool verify` or `osslsigncode verify` fails | binary | re-sign; verify before staging |

## 6. Cross-references

- `docs/release/PUBLISH_PROTOCOL.md` — governing protocol.
- `docs/release/RELEASE_TRAIN_MATRIX.md` — per-channel one-page matrix.
- `docs/release/ROLLBACK_PROCEDURE.md` — what to do if anything goes wrong.
- `docs/release/sha_ledger/README.md` — ledger lifecycle.
- `docs/release/SIGNING_MACOS.md`, `SIGNING_WINDOWS.md` — signing runbooks.
- `docs/release/SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` — multi-channel runbook for *subsequent* betas (after credentials are provisioned).
- `docs/release/templates/PREPUBLISH_AUDIT_REPORT_TEMPLATE.md`,
  `POST_PUBLISH_VERIFY_TEMPLATE.md`,
  `REMEDIATION_REPORT_TEMPLATE.md` — report scaffolds.

## 7. Ready for staged beta publish — yes/no per channel

The §0.5 preflight + the credential inventory + the existing tooling state
combine into a single yes/no answer per channel. This is the answer to
"can I publish channel X today without further setup?"

| channel | ready today? | rationale | go-to runbook section |
|---|---|---|---|
| **crates.io** (Rust SDK 32 crates) | 🟢 **YES** | token in `~/.cargo/credentials.toml`, topo dry-run + audit scripts ready, SBOM baselines committed | §1.1 — start with `formcalc-interpreter beta.8` per the chain order |
| **npm `@pdfluent/sdk-wasm`** (WASM SDK) | 🟢 **YES** | `[redacted]` owner-verified on `@pdfluent`, `wasm-pack` available, `wasm_dry_run.sh` ready | §1.2 — build pkg + audit + publish |
| **npm `@pdfluent/node`** (NAPI binding) | 🟢 **YES** (with caveat) | same npm token; pdf-node `npm pack` works (15 KB per Tier-1 CI logs). Caveat: confirm whether `pdf-node` ships JS-only-glue or includes prebuilt natives (see §1.3 step 2) | §1.3 |
| **PyPI `pdfluent`** | 🟢 **YES** | `pypi-token` in keychain, `pypi-` prefix validated, `maturin` available | §1.4 — TestPyPI first |
| **Maven Central `com.pdfluent:pdfluent`** | 🟢 **YES** | `~/.m2/settings.xml` server `central` present, GPG key `DA87…B513` for `<hello@pdfluent.com>` rsa4096 valid until 2028-05-13. **Caveat:** `pinentry-mac` not currently installed — GPG passphrase prompts will appear in the operator's terminal during `mvn deploy`. If running headless / for CI, first `brew install pinentry-mac` and configure `~/.gnupg/gpg-agent.conf` accordingly. | §1.5 — Sonatype staging first |
| **NuGet `PDFluent`** | 🟢 **YES** | `nuget-api-key` in keychain, `oy2dt…` v3 format validated, `dotnet pack` available | §1.6 |
| **C-ABI tarball `pdf-capi`** | 🟢 **YES** (unsigned) | `package_cabi.sh` ready; distribution storage destination is the only open choice (GitLab Releases, Hetzner Storage Box, etc.) | §1.7 |
| **Binary release — Linux musl** (`x86_64-unknown-linux-musl`) | 🟢 **YES** (GPG-signed) | Tier-1 CI artefact present; `binary_release.md` checklist supports GPG `.asc` Linux signing via the PDFluent GPG key already in keychain. No platform code-signing identity required for Linux | §1.8 — Linux row |
| **Binary release — Windows** (`x86_64-pc-windows-gnu`) | 🔴 **NO** | Tier-1 CI artefact present, but no code-signing identity provisioned. SmartScreen will warn on every consumer download. **Block on §3.7**: Microsoft Trusted Signing (preferred) or USB EV cert | §3.7 + `SIGNING_WINDOWS.md` |
| **Binary release — macOS** (`x86_64-apple-darwin` + `aarch64-apple-darwin`) | 🔴 **NO** | No `Developer ID Application` cert in keychain (only `Apple Development` debug certs); no notarytool API key stored. Gatekeeper will refuse to open the binary. **Block on §3.6**: Apple Developer Program + Dev ID cert + notarytool key | §3.6 + `SIGNING_MACOS.md` |
| **GitLab Releases** (artefact upload) | 🟢 **YES** | `claude-pdfluent-api` PAT in keychain, scope `api`, expires 2027-05-28 | §1.8 footer |

**Summary:** 8 of 10 publish targets are ready for staged beta publish
today, gated only on the operator's explicit per-step approval. The 2
binary-release targets requiring code-signing (Windows, macOS) need an
out-of-band one-time identity-provisioning step before they can be
publicly distributed; the unsigned tarballs can still be uploaded as
`*-unsigned.tar.gz` to internal mirrors for staging, but per the
existing `binary_release.md` Failure Modes section "Unsigned artifact
shipped" is "most catastrophic" — don't promote unsigned binaries to
the public download URL.

The CI side is NOT wired for any of the 8 ready channels (0 GitLab
project CI/CD variables provisioned) — every publish is **operator-on-
Mac**. Wiring the 5–6 secrets into GitLab CI/CD is a separate step that
unlocks CI-driven publish for the 2nd beta onward (per
`SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`).
