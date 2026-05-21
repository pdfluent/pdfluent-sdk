# PDFluent non-XFA SDK — Publish-Train Runbook & Go/No-Go Checklist

- Status: **RUNBOOK ONLY — DO NOT PUBLISH.** No commands in this document have been run.
- Source state: `SDK_NON_XFA_RELEASE_CANDIDATE_READY` merged at `enterprise/ga-hardening` @ `59426b8f7`.
- Audience: the human release operator. Every publish step requires explicit human action.
- Scope: the 7 non-XFA distribution channels. XFA remains **experimental, feature-gated, not
  production-supported** — do not represent it otherwise anywhere in release notes or store text.

> ⚠️ This runbook does not bump versions, create tags, or publish. It is the plan to follow once a
> human gives go. The artifacts audited locally were arm64-darwin / dev-host builds; **every real
> publish artifact must be (re)built on the correct release runner and re-audited** (see §2, §4).

---

## 0. Pre-conditions (must all be true before starting)

- `enterprise/ga-hardening` is green: `cargo metadata`, `fmt`, `check`, `clippy -D warnings`,
  `local_ci_gate --fast`, quality recheck (7 green), perf budgets (0 regressions),
  `audit-all-packages --dry-run` (6/6).
- The final RC audit report is merged:
  `benchmarks/runs/ga_readiness_3d/sdk_non_xfa_final_release_candidate_artifact_audit/`.
- Release credentials are provisioned (see §3 per channel) and stored only in the operator's
  secret manager — never in the repo.
- A decision has been made on the **cross-channel version policy** (see §5; versions are currently
  NOT unified).

---

## 1. Publish order (strict, top to bottom)

Publish channels in this order so that downstream consumers can resolve dependencies and so the
hardest-to-reverse channel (crates.io) is validated first:

1. **crates.io** (32 crates, topological — see §2.1)
2. **C-ABI** release tarball (the native that several other channels embed)
3. **npm** (`@pdfluent/node`)
4. **PyPI** (`pdfluent`)
5. **NuGet** (`PDFluent`)
6. **Maven Central** (`com.pdfluent:pdfluent`)
7. **WASM** (`@pdfluent/sdk-wasm`)

Rationale: crates.io is irreversible (yank-only) and is the source of truth for the Rust core;
publish and verify it first. C-ABI/native precede the binding channels that ship that native.

---

## 2. Per-channel procedures

For every channel: **(a) build on the correct runner → (b) audit the built artifact → (c) publish
(human-run) → (d) verify.** Commands marked `# DO NOT RUN until go` are the publish commands.

### 2.1 crates.io (Rust, 32 crates)

- **Runner/platform:** Linux x86_64 with `cargo-local-registry` installed and network to crates.io.
- **Build/validate (no publish):**
  ```bash
  # clean tree, exact release commit
  python3 scripts/release/topological_cratesio_dry_run.py --out /tmp/topo_result.json
  # expect: "32/32 ok, 0 failed", 0 blocking content/license issues
  ```
- **Pre-publish audit:** the harness above is the audit (per-`.crate` LICENSE/corpus/private-path).
  Additionally `cargo publish -p <crate> --dry-run` per crate as cargo's own final check.
- **Publish (human-run, in the proven topological order):**
  ```bash
  # DO NOT RUN until go. Publish one at a time; wait for index availability between each.
  cargo publish -p pdf-font            # 1.0.0-beta.3
  cargo publish -p pdfluent-ccitt      # 0.2.2
  cargo publish -p pdfluent-cff        # 0.2.0
  cargo publish -p pdfluent-jpeg2000   # 0.3.3
  cargo publish -p xfa-dom-resolver    # 1.0.0-beta.8
  cargo publish -p xfa-js-sandboxed    # 1.0.0-beta.8
  cargo publish -p xfa-license         # 1.0.0-beta.3
  cargo publish -p formcalc-interpreter# 1.0.0-beta.8
  cargo publish -p pdfluent-jbig2      # 0.2.3
  cargo publish -p pdf-syntax          # 0.5.4
  cargo publish -p pdfluent-lopdf      # 0.39.2
  cargo publish -p xfa-layout-engine   # 1.0.0-beta.8
  cargo publish -p pdf-annot           # 1.0.0-beta.4
  cargo publish -p pdf-compliance      # 1.0.0-beta.8
  cargo publish -p pdf-interpret       # 0.5.6
  cargo publish -p pdf-invoice         # 1.0.0-beta.3
  cargo publish -p pdf-ocr             # 1.0.0-beta.3
  cargo publish -p pdfluent-extract    # 1.0.0-beta.8
  cargo publish -p pdfluent-forms      # 1.0.0-beta.8
  cargo publish -p pdfluent-sign       # 1.0.0-beta.8
  cargo publish -p xfa-json            # 1.0.0-beta.8
  cargo publish -p pdf-docx            # 1.0.0-beta.8
  cargo publish -p pdf-pptx            # 1.0.0-beta.3
  cargo publish -p pdf-render          # 1.0.0-beta.8
  cargo publish -p pdf-xfa             # 1.0.0-beta.8
  cargo publish -p pdf-xlsx            # 1.0.0-beta.3
  cargo publish -p pdf-engine          # 1.0.0-beta.8
  cargo publish -p pdf-manip           # 1.0.0-beta.8
  cargo publish -p xfa-cli             # 1.0.0-beta.3
  cargo publish -p pdf-redact          # 1.0.0-beta.8
  cargo publish -p pdf-text-format     # 1.0.0-beta.8
  cargo publish -p pdfluent            # 1.0.0-beta.8   (umbrella — LAST)
  ```
- **Credentials:** `CARGO_REGISTRY_TOKEN` with publish rights; first-publish of a new crate name
  requires the owner to have claimed that name on crates.io.
- **Rollback:** crates.io is **append-only**. There is NO delete — only `cargo yank` (hides from new
  resolution; existing Cargo.lock keeps using it). Treat each publish as permanent.
- **Post-publish verify:** `cargo search <crate>` shows the version; in a scratch project
  `cargo add pdfluent@1.0.0-beta.8 && cargo build` resolves the full graph from crates.io.

### 2.2 C-ABI (release tarball)

- **Runner/platform:** one runner per target OS/arch (linux-x64 proven; add macOS/Windows as needed).
- **Build:** `bash scripts/release/package_cabi.sh` → `target/release/pdfluent-capi-1.0.0-beta.1.tar.gz`
  (contains `lib/libpdf_capi.{so,dylib,dll}` + `include/pdfluent.h` + `include/pdf_engine.h` + LICENSE).
- **Pre-publish audit:** `python3 scripts/release/check_release_artifact_contents.py <tarball>`
  (expect `ok:true`, LICENSE present, 0 corpus/secret/private-path).
- **Publish (human-run):** there is no registry — attach the per-OS tarballs to the release
  distribution location your team uses (object storage / customer download).
  `# DO NOT RUN until go: upload pdfluent-capi-<ver>-<os>-<arch>.tar.gz`
- **Credentials:** distribution storage credentials only.
- **Rollback:** replace/remove the uploaded file (reversible, unlike registries).
- **Post-publish verify:** download the tarball on a clean machine; compile the §2-style tiny C
  program against `include/` + link the lib; open a valid PDF → page count; malformed → non-zero status.

### 2.3 npm (`@pdfluent/node`, currently 1.0.0-beta.5)

- **Runner/platform:** per-OS Node runners to produce prebuilt `.node` addons (linux-x64, darwin-arm64,
  darwin-x64, win-x64); napi multi-platform packaging.
- **Build:** `napi build --platform --release` then `npm pack` per platform (and the loader package).
- **Pre-publish audit:** `python3 scripts/release/check_release_artifact_contents.py <tgz>` +
  `npm publish --dry-run` to confirm the file list.
- **Publish (human-run):** `# DO NOT RUN until go: npm publish --access public` (per platform package).
- **Credentials:** npm automation token with publish rights to the `@pdfluent` org.
- **Rollback:** `npm unpublish` is restricted (>72h or with dependents is disallowed); use
  `npm deprecate` instead. Treat as effectively permanent.
- **Post-publish verify:** `npm install @pdfluent/node@<ver>` in a scratch project; valid open →
  `pageCount`; malformed → typed `Error`.

### 2.4 PyPI (`pdfluent`, currently 1.0.0b7)

- **Runner/platform:** manylinux container (cibuildwheel) + macOS (arm64/x64) + Windows runners to
  build the full wheel matrix; plus an sdist.
- **Build:** `maturin build --release` per interpreter/arch; `cibuildwheel` for manylinux.
- **Pre-publish audit:** `python3 scripts/release/check_release_artifact_contents.py <whl>` +
  `twine check dist/*`.
- **Publish (human-run):** `# DO NOT RUN until go: twine upload dist/*`
- **Credentials:** PyPI API token (`__token__`).
- **Rollback:** PyPI does not allow re-upload of the same version after deletion; deletion is
  discouraged. Treat as permanent; yank with a post-release if needed.
- **Post-publish verify:** in a matching cp311 venv, `pip install pdfluent==<ver>`; valid open →
  `page_count`; malformed → typed error. (Local dev-host smoke was blocked only by interpreter-tag
  mismatch — must be re-smoked on a matching interpreter here.)

### 2.5 NuGet (`PDFluent`, currently 1.0.0-beta.6)

- **Runner/platform:** a runner (or CI matrix) where the native libs for each RID are present.
- **Build:**
  ```bash
  # stage natives for every shipped RID (linux-x64 proven; add win-x64/osx-x64/osx-arm64)
  CAPI_LINUX_X64=... CAPI_WIN_X64=... CAPI_OSX_X64=... CAPI_OSX_ARM64=... \
    bash scripts/release/stage_dotnet_natives.sh
  dotnet build -c Release && dotnet pack -c Release -o out
  ```
- **Pre-publish audit:** `python3 scripts/release/check_release_artifact_contents.py out/PDFluent.<ver>.nupkg`
  + confirm `runtimes/<rid>/native/` contains every intended RID.
- **Publish (human-run):** `# DO NOT RUN until go: dotnet nuget push out/PDFluent.<ver>.nupkg -s nuget.org -k $NUGET_API_KEY`
- **Credentials:** `NUGET_API_KEY`.
- **Rollback:** NuGet allows unlist (hides) but not hard delete. Treat as permanent.
- **Post-publish verify:** scratch console app, `dotnet add package PDFluent --version <ver>`; valid
  open → `PageCount`; malformed → typed `PdfException`; native auto-loads on each target RID.

### 2.6 Maven Central (`com.pdfluent:pdfluent`, currently 1.0.0-beta.6)

- **Runner/platform:** JDK 17/21 build runner (the `jdk24-plus` profile auto-activates only on
  JDK 24+); native `libpdf_capi` available for the target OSes the jar will load via JNA.
- **Build:** `mvn -P release -DskipTests package` (produces main + sources + javadoc jars; GPG-signs).
- **Pre-publish audit:** `jar tf target/pdfluent-<ver>.jar | grep -i license` (expect `META-INF/LICENSE`)
  + `python3 scripts/release/check_release_artifact_contents.py target/pdfluent-<ver>.jar`.
- **Publish (human-run):** `# DO NOT RUN until go: mvn -P release deploy` (to OSSRH staging, then
  release the staging repository).
- **Credentials:** OSSRH (`ossrh` server) username/token in `~/.m2/settings.xml`; GPG signing key +
  passphrase in gpg-agent.
- **Rollback:** Central is **immutable** once released from staging; you can drop a staging repo
  *before* release, never after. Verify in staging first.
- **Post-publish verify:** scratch Maven project depends on `com.pdfluent:pdfluent:<ver>`; valid open →
  `getPageCount`; malformed → typed `PdfluentException`.

### 2.7 WASM (`@pdfluent/sdk-wasm`, currently 1.0.0-beta.3)

- **Runner/platform:** any runner with the wasm32 target + wasm-pack (platform-independent output).
- **Build/validate:** `bash scripts/release/wasm_dry_run.sh` (expect `R1_3_WASM_DRY_RUN_GREEN`).
  Refresh the A2 SHA-256 anchor for the published version so Gate E is strict again.
- **Pre-publish audit:** the dry-run is the audit (0 blockers, LICENSE present, expected file list,
  WASM SHA byte-identity vs the version anchor).
- **Publish (human-run):** `# DO NOT RUN until go: npm publish --access public` (from `crates/xfa-wasm/pkg`).
- **Credentials:** npm automation token for `@pdfluent`.
- **Rollback:** same npm constraints as §2.3 (unpublish restricted; use deprecate).
- **Post-publish verify:** browser harness (Playwright) import/init + valid/malformed; the
  representative WASM runtime is the browser, not node.

---

## 3. Required credentials / secrets (summary)

| Channel | Secret | Notes |
|---------|--------|-------|
| crates.io | `CARGO_REGISTRY_TOKEN` | + crate-name ownership for first publish |
| npm / WASM | npm automation token (`@pdfluent` org) | publish access |
| PyPI | PyPI API token (`__token__`) | per-project token preferred |
| NuGet | `NUGET_API_KEY` | nuget.org push key |
| Maven Central | OSSRH user/token + GPG key/passphrase | `~/.m2/settings.xml` + gpg-agent |
| C-ABI | distribution storage credentials | object store / download host |

Never commit secrets. None are in the repo.

---

## 4. Native runner matrix (release-blocking for native channels)

Local audit artifacts were **arm64-darwin only**. Before publishing native-bearing channels, the
native libs MUST be built on the correct runners and the package re-audited:

| Channel | linux-x64 | win-x64 | osx-arm64 | osx-x64 | wasm32 |
|---------|-----------|---------|-----------|---------|--------|
| C-ABI | required | optional* | optional* | optional* | n/a |
| npm | required | recommended | required | recommended | n/a |
| PyPI | manylinux required | recommended | required | recommended | n/a |
| NuGet | required (proven) | recommended | optional | optional | n/a |
| WASM | n/a | n/a | n/a | n/a | required |

*Ship the RID set the product commits to. Each shipped RID must be a real build on that platform —
never hand-edited or copied across arches.

---

## 5. Cross-channel consistency checklist

| Item | Current state | Action before publish |
|------|---------------|-----------------------|
| **Versions** | NOT unified: crates.io `1.0.0-beta.8`; jar/nuget `1.0.0-beta.6`; npm `1.0.0-beta.5`; pypi `1.0.0b7`; wasm `1.0.0-beta.3`; cabi `1.0.0-beta.1` | **Decide policy** (see below). This is a go/no-go item, not auto-resolved here (no version bump allowed in this milestone). |
| Release notes | not yet drafted | draft per-channel notes from the RC audit + this train |
| Changelog | not yet cut | add a CHANGELOG entry per channel/version |
| Tags | none (do NOT create here) | operator creates annotated tags at publish time |
| License files | present in every artifact (RC audit Phase 3) | re-confirm on CI-built artifacts |
| XFA caveats | README + crate description label XFA experimental/feature-gated | keep verbatim; do not soften |

**Version-policy decision (required):** the channels intentionally carry independent package
versions today. Either (a) accept independent per-channel versions and document the mapping, or
(b) unify to a single marketing version in a *separate* version-bump milestone (NOT here). Publishing
with mismatched versions is allowed only if (a) is explicitly chosen and documented in release notes.

---

## 6. Final Go / No-Go checklist

### Release-blocking (must be GREEN to publish)
- [ ] Release commit is on `enterprise/ga-hardening`, clean tree, all gates green.
- [ ] crates.io topological dry-run re-run on the **exact** release commit → 32/32, 0 blocking.
- [ ] Native libs (re)built on the correct runner for **every shipped RID** (§4) — no cross-arch copies.
- [ ] Each built artifact re-audited with `check_release_artifact_contents.py` → `ok:true`,
      LICENSE present, 0 corpus/secret/private-path.
- [ ] PyPI wheel re-smoked on a **matching** interpreter (the dev-host BLOCKED_LOCAL must be cleared).
- [ ] All credentials present and scoped (§3).
- [ ] Version policy decided and documented (§5).
- [ ] XFA experimental/feature-gated caveats intact in every public surface.
- [ ] Human release authority has signed off in writing.

### Advisory (should be done; not strictly blocking)
- [ ] Release notes + CHANGELOG drafted per channel.
- [ ] Annotated git tags prepared (created at publish time, not before).
- [ ] WASM A2 SHA anchor refreshed for the published version.
- [ ] Post-publish verification scripts staged for each channel (§2 verify steps).

---

## 7. DO NOT PUBLISH UNTIL …

Do **not** run any publish command until **all** of the following hold:

1. A human with release authority has explicitly approved this exact release commit.
2. The crates.io topological dry-run passed on that exact commit (not a stale run).
3. Every native channel's artifact was built on the correct OS/arch runner (not the arm64-darwin
   dev host) and re-audited clean.
4. The PyPI wheel was re-smoked on a matching interpreter.
5. The cross-channel version policy (§5) is decided and written into the release notes.
6. All registry credentials are confirmed working via `--dry-run` / staging where supported.

Until then this is a **plan only**. crates.io, npm, PyPI, NuGet, and Maven Central publishes are
effectively irreversible — there is no undo, only yank/unlist/deprecate.
