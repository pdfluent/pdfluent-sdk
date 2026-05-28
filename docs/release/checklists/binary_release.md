# Channel checklist — binary release artifacts

Governed by `docs/release/PUBLISH_PROTOCOL.md`. Applies to signed installers, standalone CLI binaries, desktop app bundles, and any other compiled artifact distributed directly to end users (e.g. via a release page or signed download URL). Audit report goes to `benchmarks/runs/prepublish_audits/<artifact>-<version>.md`.

## Prepublish audit

- [ ] `git status --porcelain` empty.
- [ ] Build inputs are fully committed (no local-only changes).
- [ ] Build is reproducible from the recorded commit hash; capture `git rev-parse HEAD` in the audit report.
- [ ] Signing keys / certificates are loaded and verified (e.g. `codesign -v`, GPG list-secret-keys, EV cert chain check).
- [ ] Version string baked into the binary matches the release version (run `<binary> --version` after build).

## Build commands

Per platform; capture the exact command line in the audit report. Examples:

```
# Linux CLI
cargo build --release -p xfa-cli
strip target/release/xfa-cli

# macOS .app + signed .dmg
tauri build --release
codesign --deep --force --options runtime --sign "Developer ID Application: …" target/release/bundle/macos/<App>.app
xcrun notarytool submit …

# Windows .exe + signed installer
cargo build --release --target x86_64-pc-windows-msvc
signtool sign /tr http://timestamp.example /td sha256 /fd sha256 /a target/x86_64-pc-windows-msvc/release/<app>.exe
```

## Package content inspection

- [ ] Unpack / mount the installer artifact (`tar tzf`, `7z l`, `hdiutil attach`, `unzip -l`).
- [ ] Run `scripts/release/audit_package_tree.py` against the unpacked tree.
- [ ] Verify no `.pdb` / `.dSYM` / source map files ship unless explicitly intended.
- [ ] Verify the binary `strings` output has no absolute workstation paths.
- [ ] Size sanity: record the size; flag ≥ 2× last release as a stop-the-line trigger.

## Licence verification

- [ ] The bundled artifact contains a `LICENSE` (or dual files) visible in the installed location and/or the "About" dialog.
- [ ] Third-party licences for statically-linked deps are aggregated in a `THIRD_PARTY_LICENSES` / `licenses/` directory.
- [ ] Acknowledgements include any required attribution (BSD-style, MPL, LGPL).
- [ ] If the binary embeds proprietary models / fonts under a separate licence, that licence ships next to it.

## Signing verification

The signing *procedure* (identity acquisition, signing command, notarization,
verification commands, CI integration) lives in:

- `docs/release/SIGNING_MACOS.md` — Apple Developer ID + notarytool.
- `docs/release/SIGNING_WINDOWS.md` — Microsoft Trusted Signing (preferred);
  Azure Artifact Signing; USB-token EV fallback; OV fallback.

This checklist *verifies* the result of those runbooks:

- [ ] **macOS**: `codesign --verify --deep --strict <app>` returns no error; `spctl -a -v <app>` reports "accepted source=Notarized Developer ID".
- [ ] **Windows**: `signtool verify /pa /v <exe>` (or `osslsigncode verify <exe>` on a non-Windows host) returns success with a non-empty signer certificate AND a valid RFC-3161 timestamp.
- [ ] **Linux**: GPG detached signature `.asc` accompanies the artifact; `gpg --verify <artifact>.asc <artifact>` succeeds with the release key.
- [ ] Signature key/cert thumbprint **and** timestamp-server URL recorded in the audit report.
- [ ] `sha_ledger/binary.json` entry for this artefact is appended **after** signing — the ledger sha256 is the sha256 of the *signed* binary, which is what consumers actually download.

## Reproducibility check

- [ ] Build twice from the same commit; sha256 of the unsigned binary must match (signed binaries differ due to signature timestamps — strip signatures before comparing).
- [ ] If reproducibility is not yet achieved, document why and what would be required.

## Dependency check

- [ ] No yanked Rust deps in the build (`cargo tree | grep -i yank`).
- [ ] For Tauri / Electron apps, no deprecated transitive npm deps that we own.

## Dry-run / staging

Binary release flows usually don't have a literal `--dry-run`. Substitute:

- [ ] Upload the candidate to a **staging release page** (private repo, GitLab pre-release, or signed-but-unannounced URL).
- [ ] Run the post-publish verification steps below against the staging URL.
- [ ] Only then promote to the public release.

## Publish (= release upload) commands

```
# GitLab Release (preferred — GitHub is not available)
glab release create <tag> --name "<release name>" --notes-file release-notes.md
glab release upload <tag> path/to/artifact1 path/to/artifact1.asc …

# Internal signed download URL: copy to the signed-CDN bucket via the operator's tooling.
```

## Post-publish verification

- [ ] Download every advertised artifact from the public URL.
- [ ] Re-verify signature(s) on the downloaded copy.
- [ ] sha256 of the downloaded artifact matches the local artifact.
- [ ] Smoke test installs on a fresh VM / clean user account for each platform.
- [ ] Run `<binary> --version` and confirm version string.
- [ ] Record PASS in the audit report and update `docs/release/public_beta_release_notes.md`.

## Rollback / yank / remediation

- For a defective binary release: take down the artifact from the release page, post a notice, ship a fixed version, and instruct users to upgrade.
- Record the defect, the take-down time, and the replacement version in the remediation report.
- If the defect involves a security issue, follow `SECURITY.md` disclosure procedure.

## Failure modes specific to binary releases

- **Unsigned artifact shipped**. Most catastrophic: users hit Gatekeeper / SmartScreen warnings. Take down immediately and re-sign.
- **Wrong arch shipped**. e.g. x86_64 binary uploaded under an aarch64 filename. Smoke-test on the target platform before announcement.
- **Embedded keys / certs**. A binary that statically embeds a private signing key is a security incident — rotate the key, take down, file an internal incident.
- **Telemetry endpoint leak**. A debug build with a live telemetry URL pointing to a staging host can dox internal infrastructure. Strings-scan the binary for known internal hostnames before release.
