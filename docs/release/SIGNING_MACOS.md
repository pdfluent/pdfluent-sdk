# PDFluent — macOS Code-Signing Runbook (Apple Developer ID + Notarization)

**Status:** mandatory for any macOS binary distributed outside the Mac App
Store · **Owner:** release captain · **Started:** 2026-05-28 (Tier-3).

Operator runbook for signing and notarizing the `pdfluent` CLI binary
(and any future `.dmg` / `.pkg` installers) so macOS Gatekeeper accepts
the download. The runbook is **documentation** — no real signing happens
without the operator-provided Apple Developer ID Application certificate
and notarytool credentials.

---

## 1. Why sign + notarize

A user who downloads an unsigned binary from the web sees:

> **"pdfluent" cannot be opened because the developer cannot be verified.
> macOS cannot verify that this app is free from malware.**

After **signing** (with a valid Developer ID Application certificate),
the message changes to a less-scary "unidentified developer; right-click
and Open." After **notarization** (Apple-side malware scan), Gatekeeper
accepts the binary silently — no warning, no right-click ritual.

Both steps require Apple Developer Program membership ($99 USD/yr) and
are mandatory for any binary distributed outside the Mac App Store —
including a CLI shipped via a tarball link, Homebrew, or a `curl | sh`
installer.

## 2. Identity acquisition

| step | what | one-time? |
|---|---|---|
| 1 | Enroll the publishing entity in the **Apple Developer Program** at https://developer.apple.com/enroll/ ($99 USD/yr; corporate enrollment requires a D-U-N-S number) | yes |
| 2 | In Xcode → Preferences → Accounts, add the Apple ID for the publishing entity; click "Manage Certificates…"; create a **Developer ID Application** certificate (the right one; *not* "Developer ID Installer" unless shipping `.pkg`) | yes (renews every 5 years; renewable from same UI) |
| 3 | Verify the certificate is in the macOS keychain on the dev/CI host: `security find-identity -v -p codesigning` — must list "Developer ID Application: <Org> (TEAMID)" with a non-revoked status | each host that signs |
| 4 | Create a **Notarytool API key** at https://appstoreconnect.apple.com → Users and Access → Integrations → App Store Connect API → Keys. Key type: **Developer** (sufficient for notarytool; do not use Admin). Download the `.p8` (one-time; cannot be re-downloaded). Record the `KeyID` and `Issuer ID`. | yes |
| 5 | Store the `.p8`, `KeyID`, and `Issuer ID` in macOS keychain via `xcrun notarytool store-credentials "pdfluent-notarytool" --key <p8-path> --key-id <KeyID> --issuer <IssuerID>`. From this point notarytool can be invoked with just `--keychain-profile "pdfluent-notarytool"`. | each signing host |

**Never** commit any of `.p8`, `KeyID`, `Issuer ID`, the keychain
password, or the signing identity name to the repo. These are
identity-binding secrets; check-in is a security incident.

## 3. Sign + notarize procedure (manual, per release)

Pre-conditions: the cross-built or Mac-built `pdfluent` binary exists at
a known path (e.g. `dist/x86_64-apple-darwin/pdfluent` or
`dist/aarch64-apple-darwin/pdfluent`).

```bash
BIN="dist/x86_64-apple-darwin/pdfluent"      # adjust per target
TEAMID="<your team id>"                      # see step 3 above
IDENT="Developer ID Application: <Org Name> ($TEAMID)"

# 1. Sign with hardened runtime (required for notarization).
codesign \
  --sign "$IDENT" \
  --options runtime \
  --timestamp \
  --identifier "com.pdfluent.cli" \
  --force \
  "$BIN"

# 2. Verify signature locally before notarizing.
codesign --verify --verbose=2 "$BIN"

# 3. Tar (notarytool accepts .zip, .pkg, .dmg — not bare binaries; wrap
#    the binary in a .zip first).
ditto -c -k --keepParent "$BIN" "${BIN}.zip"

# 4. Submit to Apple's notarization service.
xcrun notarytool submit "${BIN}.zip" \
  --keychain-profile "pdfluent-notarytool" \
  --wait
# `--wait` polls until the submission completes (~3–15 min). Note the
# Submission ID for the audit report.

# 5. Apple notarization passes/fails. On pass:
#    - For bare CLI binaries you can ship the .zip as-is.
#    - For .app / .pkg / .dmg, staple the ticket so Gatekeeper still
#      accepts the artefact when offline:
#        xcrun stapler staple "$BIN"        # for bundled apps/pkgs/dmgs
#    - Stapling a bare binary is not currently supported; the .zip
#      bundle wrapper is what gets the ticket attached implicitly.

# 6. Verify Gatekeeper acceptance.
spctl --assess --verbose=2 --type execute "$BIN"
# Expected: "<BIN>: accepted source=Notarized Developer ID"
```

Save:
- `codesign --display --verbose=4 "$BIN"` output (signature details)
- notarytool submission ID + log URL
- `spctl --assess` verdict line

All three go into the prepublish audit report for the macOS binary
release (`benchmarks/runs/prepublish_audits/pdfluent-<v>-macos.md`).

## 4. CI integration runbook

CI signing requires two secrets per signing host:

| secret | what | how |
|---|---|---|
| `MACOS_CERT_P12_B64` | base64-encoded `.p12` export of the Developer ID Application certificate + private key | one-time export from keychain via `Keychain Access → Export… → .p12`, then `base64 -i cert.p12 > cert.p12.b64` |
| `MACOS_CERT_P12_PASSWORD` | passphrase chosen at `.p12` export time | choose at export, store with the `.p12.b64` |
| `MACOS_NOTARYTOOL_PROFILE` | name of the keychain profile from §2 step 5 | string (e.g. `pdfluent-notarytool`) |

In CI:
1. Decode the `.p12` to disk on a private tmp path.
2. `security create-keychain -p <ephemeral-pw> build.keychain`
3. `security import cert.p12 -k build.keychain -P "$MACOS_CERT_P12_PASSWORD" -A`
4. `security list-keychains -s build.keychain login.keychain`
5. Sign + notarize as in §3.
6. **Always** `security delete-keychain build.keychain` in the CI cleanup
   step (success or failure).

This repo's `.gitlab-ci.yml` does NOT currently wire macOS signing —
**a macOS runner is not on the VPS** (Hetzner Linux only), and the
operator builds macOS binaries on a Mac. When the editor repo's macOS
CI is set up, this runbook is the operator-facing source of the steps.

## 5. Stop-the-line conditions

The signing step stops (and the release does not ship) on any of:

- `security find-identity -v -p codesigning` does not include the
  Developer ID Application identity, or the identity is revoked /
  expired.
- `codesign --verify` reports any mismatch on the just-signed binary.
- `xcrun notarytool submit --wait` returns a status other than
  `Accepted`.
- `spctl --assess` reports `rejected` for any reason.

A notarization rejection prints a log URL; download and inspect it
before re-submitting. Common rejections:

- "Hardened runtime not enabled" → add `--options runtime` to codesign.
- "Binary not signed" → resign with the correct identity.
- "Invalid timestamp" → ensure `--timestamp` was passed (Apple's timestamp
  server must be reachable from the signing host).
- "Disallowed library / unsafe symbol" → review build flags + linked
  libs; this is a binary-level fix, not a signing fix.

## 6. Verification at any later time

Any third party can verify the published macOS binary:

```bash
# Download from the registry URL recorded in sha_ledger/binary.json.
curl -L -o pdfluent.zip "<registry_url>"
unzip pdfluent.zip

# Verify the signature.
codesign --verify --verbose=2 pdfluent
# Expected line: "pdfluent: valid on disk", "pdfluent: satisfies its Designated Requirement"

# Verify notarization (network-required).
spctl --assess --verbose=2 --type execute pdfluent
# Expected: "pdfluent: accepted source=Notarized Developer ID"
```

These two commands are the definitive answer to "is this binary safe to
run on macOS." They are what Gatekeeper itself runs the first time a user
opens the binary on their Mac.

## 7. Where this fits in the publish flow

```
build (Mac, cross or native) ──▶ this signing runbook ──▶ stage to dist/
                                                            │
                                                            ▼
                                                    package:binary-release
                                                    (or the manual macOS
                                                     equivalent of it)
                                                            │
                                                            ▼
                                                    ledger_add_entry.py
                                                    (sha_ledger/binary.json)
                                                            │
                                                            ▼
                                                    smoke_binary.sh
                                                    (will skip --version
                                                     exec on a non-macOS
                                                     CI runner; structural
                                                     checks still gate)
```

## 8. Cross-references

- `docs/release/PUBLISH_PROTOCOL.md` §11, §14 — post-publish verify +
  stop-the-line conditions.
- `docs/release/checklists/binary_release.md` — the channel-level
  checklist; this runbook fills the "macOS signing" boxes in that
  checklist.
- `docs/release/SIGNING_WINDOWS.md` — the equivalent runbook for Windows
  code signing.
- `docs/release/sha_ledger/README.md` — every notarized macOS binary
  MUST be ledger-recorded (R6-1).
- Apple's authoritative docs: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution
