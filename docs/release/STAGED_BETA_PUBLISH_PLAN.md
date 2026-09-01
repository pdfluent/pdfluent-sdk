# PDFluent — Staged Beta Publish Plan (1.0.0-beta.8)

**Status:** plan only · **Owner:** release captain (jasperdew) · **Started:** 2026-05-29.
**No publish executed.** No registry-changing command runs without explicit
operator approval per channel. The plan exists so the operator can pick
exactly one channel to execute, verify the result, then come back for the
next one.

---

## 0. Pre-conditions

- [x] MR !5 (Tier-1) merged into master (`1c0814d04`).
- [x] MR !6 (Tier-2/3) merged into master (`8ad3d767f`).
- [ ] **MR !7** (`release/staged-beta-smokes` → `master`, the WASM
      `snippets/` fix) merged into master. Currently OPEN + mergeable;
      owner click required before promoting the WASM tgz beyond the
      operator-staging dir.
- [x] §0.5 preflight (FIRST_BETA_OPERATOR_RUNBOOK §0.5) PASS on the
      operator Mac for all 7 credentials.
- [x] All 7 channel smokes HARD PASS with matched runtimes (see
      `benchmarks/runs/staged_beta_smokes/`).

## 1. Channel classification

The 10 publishable channels fall into 4 buckets based on what blocks
each one today. Only **bucket A** is candidate for the first staged
publish session.

### Bucket A — safe first publish (4 channels)

These channels publish today with maximum reversibility. Pick one,
publish, verify, come back for the next.

| § | channel | reversibility |
|---|---|---|
| 1.1 | crates.io leaf `xfa-dom-resolver` | `cargo yank` (hides from new resolution; existing Cargo.lock keeps using it) |
| 1.2 | npm `@pdfluent/sdk-wasm` | within 72h: `npm unpublish`; after: `npm deprecate` |
| 1.4 | **TestPyPI only** (NOT PyPI proper) | yank via web UI; throwaway test index |
| 1.5 | **Maven Central STAGING only** (NOT release) | `mvn nexus-staging:drop` |

### Bucket B — platform-limited publish (2 channels)

Channel can publish, but the artefact only covers a subset of platforms.
A second/third build pass is needed before the channel is "complete."

| § | channel | what's covered today | what's missing |
|---|---|---|---|
| 1.4 | PyPI cp311-macos-arm64 wheel | Python 3.11 on macOS Apple Silicon | cp310/cp312/cp313 + Linux + Windows + macOS-x86_64 wheels (need cibuildwheel matrix or per-platform maturin builds) |
| 1.3 | npm NAPI main package + darwin-arm64 sub-pkg | macOS Apple Silicon native | darwin-x64, linux-x64-{gnu,musl}, linux-arm64-gnu, win32-x64-msvc (need per-platform `napi build`, see §3) |

**Do NOT publish bucket B until platform coverage is complete** — a
half-covered npm/PyPI release leaves consumers on uncovered platforms
hitting `ERR_MODULE_NOT_FOUND` or `not a supported wheel on this platform`,
which is the same defect class the matched-runtime smokes were designed
to catch.

### Bucket C — blocked until signing identity (2 channels)

| § | channel | blocker |
|---|---|---|
| 1.8 | Binary release macOS | `Developer ID Application` cert + notarytool API key (only `Apple Development` debug certs in keychain today) |
| 1.8 | Binary release Windows | Microsoft Trusted Signing OR USB-token EV cert (none provisioned) |

Both blockers are documented in `SIGNING_MACOS.md` / `SIGNING_WINDOWS.md`.
Operator-side one-time setup; not engineering work.

### Bucket D — blocked until consumer-ready (2 channels)

| § | channel | blocker |
|---|---|---|
| 1.6 | NuGet `PDFluent` | nupkg currently bundles only osx-arm64 native (`runtimes/osx-arm64/native/libpdf_capi.dylib`). The smoke PASSES on osx-arm64 hosts but consumers on win-x64, linux-x64, osx-x64 will hit `DllNotFoundException` at first `PdfDocument.Open(…)` call. Same multi-platform gap as NAPI/PyPI. |
| 1.3 | npm `@pdfluent/node` MAIN package | publishing the main package before all 6 optional `@pdfluent/node-<target>` sub-packages exist makes `npm install` succeed (the optionals are optional) but every consumer's first import fails with "Cannot find module @pdfluent/node-<their-target>". Same defect class as bucket B; bucket A's WASM analog was caught and fixed. |

---

## 2. Per-channel candidate sheet (bucket A only)

Each subsection is a single-channel "approval card". Bring it back to me
verbatim with "go §X.Y" to authorise the publish; I run the publish +
ledger + verify + smoke + stop, and we move on (or stop).

### 2.A1 — crates.io leaf `xfa-dom-resolver`

| field | value |
|---|---|
| artefact path | `target/package/xfa-dom-resolver-1.0.0-beta.8.crate` |
| sha256 | `89a9d47f5bbd9f6aca93a5528da23cd62367d386cbfabf73f2e27b674f92e560` |
| credential | `~/.cargo/credentials.toml [registry].token` (preflight: OK) |
| exact publish command | `cargo publish -p xfa-dom-resolver` |
| reversibility | `cargo yank --version 1.0.0-beta.8 xfa-dom-resolver` — hides from new resolution; existing Cargo.lock pins still resolve; the version number is **forever-taken** on crates.io |
| ledger command (after publish RC=0) | `python3 scripts/release/ledger_add_entry.py --channel crates_io --package xfa-dom-resolver --version 1.0.0-beta.8 --artifact target/package/xfa-dom-resolver-1.0.0-beta.8.crate --audit-report benchmarks/runs/prepublish_audits/xfa-dom-resolver-1.0.0-beta.8.md --registry-url https://static.crates.io/crates/xfa-dom-resolver/xfa-dom-resolver-1.0.0-beta.8.crate` |
| post-publish verify command (after 60s) | `python3 scripts/release/ledger_verify.py --channel crates_io --package xfa-dom-resolver --version 1.0.0-beta.8 --write --verify-report benchmarks/runs/post_publish_verify/xfa-dom-resolver-1.0.0-beta.8.md` |
| smoke command | `cargo new --bin /tmp/xfa-smk; (cd /tmp/xfa-smk; echo "xfa-dom-resolver = \"=1.0.0-beta.8\"" >> Cargo.toml; cargo build)` then verify build OK + index has the version |
| stop point | after `ledger_verify.py` reports `VERIFIED MATCH`. Do NOT publish the next crate in the topo chain (formcalc-interpreter etc.) without explicit go-§X.Y for it — crates.io index propagation can take 5–15 min. |

### 2.A2 — npm `@pdfluent/sdk-wasm` (POST MR !7 merge)

| field | value |
|---|---|
| artefact path | `dist/staged_beta/pdfluent-sdk-wasm-1.0.0-beta.8.tgz` |
| sha256 | `0a5bb67ca0e08246612760db50e36dcf95974fa4eb6bb817cfc47b52da060234` (post-snippets-fix) |
| credential | `~/.npmrc //registry.npmjs.org/:_authToken` (preflight: `npm whoami` returns the account that owns `@pdfluent`) |
| exact publish command | `( cd crates/xfa-wasm/pkg && npm publish --access public )` |
| reversibility | within 72h: `npm unpublish @pdfluent/sdk-wasm@1.0.0-beta.8`. After 72h: `npm deprecate "@pdfluent/sdk-wasm@1.0.0-beta.8" "<reason>"`. The version name is forever-taken either way. |
| pre-publish gate | **MR !7 must be merged into master first** — the operator should be on a master that contains commit `b3c5f82e8`, otherwise the next person re-running `transform-wasm-pkg.sh` from master regresses the `snippets/` inclusion. |
| ledger command (after publish RC=0) | `python3 scripts/release/ledger_add_entry.py --channel wasm --package @pdfluent/sdk-wasm --version 1.0.0-beta.8 --artifact dist/staged_beta/pdfluent-sdk-wasm-1.0.0-beta.8.tgz --audit-report benchmarks/runs/prepublish_audits/sdk-wasm-1.0.0-beta.8.md --registry-url https://registry.npmjs.org/@pdfluent/sdk-wasm/-/sdk-wasm-1.0.0-beta.8.tgz` |
| post-publish verify command (after 30s) | `python3 scripts/release/ledger_verify.py --channel wasm --package @pdfluent/sdk-wasm --version 1.0.0-beta.8 --write` |
| smoke command | the same matched-runtime smoke that passed locally; downloads from npm registry: `bash docs/release/consumer_smokes/smoke_wasm.sh --pkg-dir <fresh-install-dir>` |
| stop point | after smoke PASS. Do NOT touch NAPI main (`@pdfluent/node`) — bucket D. |

### 2.A3 — TestPyPI ONLY (NOT pypi.org)

| field | value |
|---|---|
| artefact path | `dist/staged_beta/pdfluent-1.0.0b8-cp311-cp311-macosx_11_0_arm64.whl` |
| sha256 | `d1bb030e208344f922cce095ea9418b0de8ac559420bb22dcba383045c1aee6e` |
| credential | keychain `pypi-token` via TWINE_PASSWORD (preflight: `pypi-` prefix OK; same token usually works for both pypi.org and test.pypi.org — confirm in your account that the token scope includes TestPyPI) |
| exact publish command | `TWINE_USERNAME=__token__ TWINE_PASSWORD="$(security find-generic-password -s pypi-token -w)" python3 -m twine upload --repository testpypi dist/staged_beta/pdfluent-1.0.0b8-cp311-cp311-macosx_11_0_arm64.whl` |
| reversibility | TestPyPI: yank via project web UI (`test.pypi.org/manage/project/pdfluent/release/1.0.0b8/`). Reversible-as-yank-but-name-stays-taken on TestPyPI. **PyPI proper is NOT touched by this step.** |
| ledger command (after upload RC=0) | `python3 scripts/release/ledger_add_entry.py --channel pypi --package pdfluent --version 1.0.0b8.test --artifact dist/staged_beta/pdfluent-1.0.0b8-cp311-cp311-macosx_11_0_arm64.whl --audit-report benchmarks/runs/prepublish_audits/pdfluent-py-1.0.0b8-testpypi.md --registry-url https://test.pypi.org/project/pdfluent/1.0.0b8/` |
| post-publish verify command | `python3 scripts/release/ledger_verify.py --channel pypi --package pdfluent --version 1.0.0b8.test --write` (note the `.test` suffix on version so the pypi.org entry stays free for the real publish) |
| smoke command (matched runtime) | use the same uv-managed arm64 Python 3.11.14 venv pattern as `benchmarks/runs/staged_beta_smokes/pypi-result.txt`, but install from TestPyPI: `<py3.11>/pip install --index-url https://test.pypi.org/simple/ pdfluent==1.0.0b8 ; <py3.11>/python -c 'import pdfluent; print(pdfluent.__version__)'` |
| stop point | after smoke confirms `pdfluent.__version__ == '1.0.0b8'` from TestPyPI install. **Do NOT publish to pypi.org until cibuildwheel matrix has built wheels for cp310/cp311/cp312/cp313 × linux/macos/windows.** |

### 2.A4 — Maven Central STAGING only (NOT release)

| field | value |
|---|---|
| artefact path | `dist/staged_beta/pdfluent-1.0.0-beta.8.jar` (+ `-sources.jar` + `-javadoc.jar`) |
| sha256 (main jar) | `47653128fd8668afca46b50546c48c727328eda5c92a8439442d74a767a61aa3` |
| credentials | `~/.m2/settings.xml` server `central` user `Wg2BVQ` + password; GPG key `DA87C8735AA58F214576C4833F2620CC58C0B513` for `<hello@pdfluent.com>` (preflight: settings OK, GPG OK). `pinentry-mac` NOT installed → operator gets a TTY passphrase prompt during signing (acceptable for first publish; install pinentry-mac before any CI-side flow). |
| exact publish command | `( cd bindings/java && mvn deploy -P release -DperformRelease=true )` — uploads to the Sonatype OSSRH **staging** repo. Does NOT release to Central. |
| reversibility | `( cd bindings/java && mvn nexus-staging:drop )` while the staging repo is open. **Fully reversible** until the operator clicks "Release" in the Sonatype web UI. |
| ledger command (after staging upload RC=0) | `python3 scripts/release/ledger_add_entry.py --channel maven --package com.pdfluent:pdfluent --version 1.0.0-beta.8.staging --artifact dist/staged_beta/pdfluent-1.0.0-beta.8.jar --audit-report benchmarks/runs/prepublish_audits/pdfluent-java-1.0.0-beta.8-staging.md --registry-url https://s01.oss.sonatype.org/service/local/repositories/<staging-id>/content/com/pdfluent/pdfluent/1.0.0-beta.8/pdfluent-1.0.0-beta.8.jar` (the `<staging-id>` is printed by `mvn deploy`; capture it) |
| post-publish verify command | `python3 scripts/release/ledger_verify.py --channel maven --package com.pdfluent:pdfluent --version 1.0.0-beta.8.staging --write` |
| smoke command (matched runtime) | same matched-runtime Java smoke pattern as the local PASS; install JNA + libpdf_capi.dylib (or libpdf_capi.so on linux) + reflect-load all `com.pdfluent.*` classes from the just-deployed jar pulled out of the staging repo URL |
| stop point | after the staging smoke PASS. **Do NOT click "Release" in the Sonatype UI** — that promotes the artefact to Maven Central which is immutable post-release. Drop the staging repo if anything is wrong; come back with a fixed version. |

---

## 3. NAPI multi-platform packaging plan

The NAPI channel is in bucket D until all 6 platform sub-packages exist.
This section says exactly what to build, where to build it, and whether
to publish the main package before all optionals exist.

### 3.1 Required optional packages

Per `crates/pdf-node/package.json` `optionalDependencies` block, the
main `@pdfluent/node@1.0.0-beta.8` package declares it would use any of
these matching-platform sub-packages:

| sub-package | platform / cpu | how built |
|---|---|---|
| `@pdfluent/node-darwin-arm64` | macOS Apple Silicon | **DONE today** — see `dist/staged_beta/pdfluent-node-darwin-arm64-1.0.0-beta.8.tgz`, sha256 `cf3456560071e8021e65194c86ea4cf263f473d2fd07cb47bf941b0a1fec4c06` |
| `@pdfluent/node-darwin-x64` | macOS Intel | cross-build (Apple Silicon Mac can target `x86_64-apple-darwin` via `cargo build --target` if Xcode SDK has the SDK installed — currently UNVERIFIED) |
| `@pdfluent/node-linux-x64-gnu` | Linux glibc x86_64 | **VPS CI** has the toolchain (Tier-1 jobs already cross-build for `x86_64-unknown-linux-musl`; gnu target is simpler — same `gcc-x86-64-linux-gnu` + rustup target) |
| `@pdfluent/node-linux-x64-musl` | Linux musl x86_64 | **VPS CI** already has this exact target wired in Tier-1 (`package:cli-cross-platform`); reuse the same `cargo build --target x86_64-unknown-linux-musl --release` step but for `pdf-node` instead of `xfa-cli` |
| `@pdfluent/node-linux-arm64-gnu` | Linux glibc aarch64 | VPS CI is x86_64; needs `aarch64-unknown-linux-gnu` rustup target + `gcc-aarch64-linux-gnu` cross-linker (one-time apt-install) |
| `@pdfluent/node-win32-x64-msvc` | Windows MSVC x64 | MSVC target needs Windows host. VPS CI cross-builds windows-**gnu** (mingw); MSVC requires either a Windows runner OR cargo-xwin (Wine-based; experimental). Recommended path: GitLab Windows runner (shared or self-hosted). |

### 3.2 What can be built on current CI today (no infrastructure change)

| sub-package | feasibility |
|---|---|
| `node-darwin-arm64` | only on a Mac (no Mac runner in CI today); built on operator Mac and staged |
| `node-darwin-x64` | only on a Mac with macOS x86_64 SDK |
| `node-linux-x64-gnu` | ✅ VPS CI — add a job mirroring `package:cli-cross-platform` for `pdf-node`, target `x86_64-unknown-linux-gnu` |
| `node-linux-x64-musl` | ✅ VPS CI — same shape, target `x86_64-unknown-linux-musl` |
| `node-linux-arm64-gnu` | ⚠️ VPS CI — needs runner-image `apt install gcc-aarch64-linux-gnu` + `rustup target add aarch64-unknown-linux-gnu` (one-time, ~5 min) |
| `node-win32-x64-msvc` | 🔴 VPS CI cannot (no Windows runner; cargo-xwin is experimental). Need GitLab Windows shared runner OR self-hosted Windows runner. |

### 3.3 Recommended phased path

1. **Phase 1 — build the two macOS sub-packages on the operator Mac** (darwin-arm64 done; darwin-x64 still TODO). 1 hour of operator work.
2. **Phase 2 — wire a new CI job `package:napi-cross-platform`** on the VPS runner that mirrors the existing Tier-1 `package:cli-cross-platform` for `pdf-node` instead of `xfa-cli`, building the three Linux sub-packages (`x64-gnu`, `x64-musl`, `arm64-gnu`). Single MR; same `.cargo/config.toml` linker setup as Tier-1. ~half-day engineering work.
3. **Phase 3 — Windows sub-package**: either provision a GitLab Windows runner (paid SaaS minutes or self-hosted), or accept that the first beta ships without Windows NAPI support. Document either way in the npm package README.
4. **Phase 4 — publish train**: publish the 6 sub-packages first (in any order — they are independent); then publish the main `@pdfluent/node` package. **Never publish main before all sub-packages exist** for the platforms you want to support, or every consumer on the missing platforms hits the same `Cannot find module @pdfluent/node-<target>` error on first import.

### 3.4 Should the main package be published before all optionals?

**No.** The main package's `optionalDependencies` block tells npm to try
each sub-package and silently ignore missing ones. The JS code in
`index.js` then tries to `require('@pdfluent/node-' + targetName)` and
THROWS if not present (the smoke proved this). Optional dependencies are
optional for npm-install purposes, **not** optional for the JS code to
actually work.

If we ship main + only `darwin-arm64`, any consumer on linux/win/darwin-x64
hits the error. There is no "graceful degradation" path. The npm package
either fully covers your platform or it doesn't run at all.

The right release pattern is the napi-rs canonical: publish each
sub-package first, then the main. That gives consumers the full set of
"my-platform-is-supported-OR-I-get-a-clean-npm-error" — never a "npm
install passed but import fails" surprise.

---

## 4. Bucket priority order (bucket A only, this session)

Recommended execution sequence:

1. **§2.A4 — Maven Central STAGING** (most reversible — `mvn deploy` to
   staging then `nexus-staging:drop` if anything is wrong; nothing
   reaches Central without the explicit web-UI release click)
2. **§2.A3 — TestPyPI only** (also reversible-ish; TestPyPI is the
   "staging" registry for pypi.org)
3. **§2.A2 — npm `@pdfluent/sdk-wasm`** (after MR !7 merge; reversible
   within 72h; smaller blast radius than crates.io because npm consumers
   typically pin loose ranges)
4. **§2.A1 — crates.io leaf `xfa-dom-resolver`** (least reversible per
   step: yank hides but the version name is forever-taken on crates.io;
   put it last in the session so you've practised the publish + ledger
   loop on more reversible channels first)

Between each: stop, run the channel's smoke, run `ledger_verify.py
--write`, commit the ledger update, push to master. Then the next.

---

## 5. What does NOT publish in this plan

- ❌ PyPI proper (pypi.org) — needs cibuildwheel matrix (bucket B)
- ❌ Maven Central release click — needs staging smoke first (bucket A
  step §2.A4 stops at staging)
- ❌ NuGet — needs multi-platform native bundling (bucket D)
- ❌ NAPI main `@pdfluent/node` — needs all 6 sub-packages (bucket D)
- ❌ Binary release Linux musl — needs CI artefact fetch (bucket B/C
  depending on signing policy)
- ❌ Binary release Windows/macOS — needs signing identities (bucket C)
- ❌ Any `v*` Git tag

## 6. Cross-references

- `docs/release/FIRST_BETA_OPERATOR_RUNBOOK.md` — full per-channel
  runbook; §1 of this plan is the single-session subset.
- `docs/release/RELEASE_TRAIN_MATRIX.md` — per-channel matrix.
- `docs/release/ROLLBACK_PROCEDURE.md` — what to do if anything goes
  wrong mid-train.
- `docs/release/sha_ledger/README.md` — ledger lifecycle; every
  publish writes an entry, every entry is verified post-publish.
- `benchmarks/runs/staged_beta_smokes/` — local smoke evidence for each
  channel (matched runtimes; all 7 PASS).
- MR !7 — the WASM `snippets/` fix; must merge before §2.A2.
