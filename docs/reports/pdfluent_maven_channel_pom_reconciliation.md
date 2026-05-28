# PDFluent Maven Channel — POM Reconciliation

- Milestone: `PDFLUENT_MAVEN_CHANNEL_POM_RECONCILIATION`
- Date: 2026-05-25 · Branch: `quality/pdfluent-maven-channel-pom-reconciliation` · Base: `enterprise/ga-hardening` @ `84d858eeb`
- **Verdict: `PDFLUENT_MAVEN_CHANNEL_POM_RECONCILIATION_GREEN_NO_PUBLISH`**
- Scope honored: no publish · no XFA behavior changes · no FreshMerge/parser changes · no corpus access · no secrets/private paths/artifacts · no license handwaving. Zero Rust/Java *source* changed (manifests, CI, scripts, docs, one new guard).

## 1. Inventory (repo evidence)

| Path | Coordinates | Version | License | Java pkg | Role |
|---|---|---|---|---|---|
| `bindings/java/pom.xml` | `com.pdfluent:pdfluent` | **1.0.0-beta.8** | LicenseRef-PDFluent-Commercial | `com.pdfluent.*` | **Canonical Java SDK** (JNA + raw-JNI, typed exceptions, NativeLoader, LICENSE bundled, release profile, jdk24 portability) |
| `crates/pdf-java/pom.xml` | `com.pdfluent:xfa-pdf` | 1.0.0-beta.1 | **MIT** | `com.xfa.pdf.*` | **Legacy/stale** Maven artifact (pre-rebrand naming + license) |
| `crates/pdf-java` (Rust crate) | cdylib `pdfluent_java` | (workspace) | — | — | **Native JNI provider** — builds `libpdfluent_java` that `bindings/java` loads. **Kept.** |
| `bindings/android/*.gradle.kts` | Android | — | — | — | Separate Android surface (out of scope) |
| `crates/xfa-test-runner/oracle/itext-xfa-oracle/pom.xml` | iText oracle | — | — | — | Test oracle only (not a product channel) |
| `pdfluent-examples/java/StrictApi/pom.xml` | example | — | — | — | Example only |

**Wiring before:** CI `package:maven-audit`, `scripts/publish_all.sh`, and `docs/release/consumer_smokes/smoke_java.sh` all targeted the **stale** `crates/pdf-java`. Meanwhile the unified `scripts/release/audit-all-packages.sh` (Java channel), QR11 runtime mapping, `check_binding_api_parity.py`, and the perf baseline already targeted **`bindings/java`**.

## 2. Canonical channel — DECIDED from evidence (no decision required)

**Canonical = `bindings/java` (`com.pdfluent:pdfluent`).** Evidence:
1. Native-lib match: the Rust crate `pdf-java` produces cdylib **`pdfluent_java`**, which `bindings/java/NativeLoader` loads via `System.loadLibrary("pdfluent_java")`. The legacy `xfa-pdf` pom only recompiles the superseded `com.xfa.pdf` Java classes against that same lib.
2. Current naming + license: `com.pdfluent` package, `pdfluent` artifactId, **commercial** license, bundled `LICENSE`. The legacy path is pre-rebrand `com.xfa.pdf` / `xfa-pdf` / **MIT**.
3. The unified package auditor, QR11, API-parity, and perf tooling already treat `bindings/java` as the Java channel.
4. The legacy `com.xfa.pdf` API is **dead**: no Java source imports it; the only stray reference was a wrong import in `docs/licensing.md` (the class actually lives in `com.pdfluent`).

## 3. Reconciliation plan (per path)

| Aspect | `bindings/java` (canonical) | `crates/pdf-java/pom.xml` (deprecated) |
|---|---|---|
| Release eligibility | Publishable (Maven Central, `-P release`) | **Non-publishable** (deploy disabled) |
| artifactId/groupId | `com.pdfluent:pdfluent` ✓ | `com.pdfluent:xfa-pdf` (frozen, deprecated) |
| License | Commercial ✓ | MIT (legacy; harmless now it cannot publish) |
| Version source | beta.8 (RC line) ✓ | beta.1 (frozen; not on RC line by design) |
| Package contents | jar + sources + javadoc, `META-INF/LICENSE` | n/a (not deployed) |
| CI coverage | `package:maven-audit` (repointed) + `audit-all` | change-trigger only |
| Publish integration | `publish_all.sh` (`-P release deploy`) | removed |
| Blocker status | none | resolved (deprecated + guarded) |

## 4. Changes implemented (no publish)

1. **Deprecated + neutralized `crates/pdf-java/pom.xml`:** removed `distributionManagement` + `nexus-staging-maven-plugin` + always-on GPG; added `maven-deploy-plugin <skip>true</skip>`; marked `<name>/<description>` DEPRECATED with a banner pointing to the canonical channel. `mvn deploy` can no longer publish `xfa-pdf`.
2. **Repointed CI `package:maven-audit`** → builds/audits `bindings/java`; trigger now fires on `bindings/java/**` and `crates/pdf-java/**`; added the guard as the first step; **fixed the audit invocation** to the correct `audit_package_tree.py --tree/--out/--package-name/--package-version --channel maven` form (the prior positional form was a pre-existing breakage — see §7).
3. **Repointed `scripts/publish_all.sh`** Maven step → `cd bindings/java && mvn -P release deploy` (still `--dry-run`), guarded.
4. **Repointed `docs/release/consumer_smokes/smoke_java.sh`** → artifactId `pdfluent`, path `bindings/java`, version fallback `beta.8`.
5. **Added `scripts/release/maven_channel_guard.sh`** (fail-closed): asserts canonical identity/license/RC-version, that the deprecated pom carries no publish plumbing and hard-skips deploy, and that no script `mvn deploy`s the deprecated path. Wired into CI + `publish_all.sh`.
6. **Fixed stale `com.xfa.pdf` import** in `docs/licensing.md` → `com.pdfluent`.

## 5. Validation

| Check | Result |
|---|---|
| `maven_channel_guard.sh` (local + VPS) | **PASS** (8/8 assertions) |
| `mvn package -DskipTests` on `bindings/java` (VPS, JDK 21) | **PASS** → `pdfluent-1.0.0-beta.8.jar` (+ sources, javadoc) |
| `audit_package_tree.py --channel maven` on built tree (VPS) | **PASS** — 0 blockers, 0 warnings |
| Both poms well-formed XML | OK |
| `.gitlab-ci.yml` YAML valid | OK |
| `check_release_consistency.py --offline` | PASS (20 crates beta.8, no drift) |
| `check_no_private_paths.sh` | OK |
| Leak scan (changed files) | clean (the `/mnt/storagebox` hits are pre-existing CI cache mounts, not in this diff) |
| Branch pipeline (pre-push local CI gate) | **PASS** (metadata/fmt/build/clippy 4/4) |
| No publish · no Rust/XFA source changed | confirmed |

## 6. Acceptance criteria — met

- ✅ One canonical Maven path clearly established (`bindings/java`); roles separated (Rust `pdf-java` = native lib; legacy pom = deprecated/frozen).
- ✅ Stale path cannot accidentally publish (deploy skip + no dist-mgmt + guard).
- ✅ Metadata/license/version coherent (`com.pdfluent:pdfluent`, commercial, beta.8).
- ✅ dry-run/package audit passes (0 blockers).
- ✅ No publish.
- ✅ Branch pipeline green; merge pipeline to run on integration.

## 7. Notes / out-of-scope findings

- **Pre-existing audit-invocation breakage (other channels):** `package:python-wheel-audit` and `package:nuget-audit` call `audit_package_tree.py <path>` positionally, which the current script rejects (requires named args). These manual jobs would error if run. Out of scope here (fixed only the Maven job); recommend a small follow-up to repair the python/nuget invocations the same way.
- The legacy `crates/pdf-java/java/com/xfa/pdf/*.java` sources are retained (deprecated, non-published) rather than deleted, to keep the change minimal and reversible. A future cleanup could remove them entirely once confirmed unreferenced by any consumer.

## 8. Recommended next milestone

**`PDFLUENT_PACKAGE_AUDIT_INVOCATION_REPAIR`** — fix the pre-existing positional `audit_package_tree.py` calls in the python-wheel-audit and nuget-audit CI jobs (and any others) to the required `--tree/--out/--package-name/--package-version --channel` form, so all per-channel package audits actually run. No publish.
