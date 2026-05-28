# PDFluent — Windows Code-Signing Runbook (Microsoft Trusted Signing + Fallbacks)

**Status:** mandatory for any Windows binary distributed publicly · **Owner:**
release captain · **Started:** 2026-05-28 (Tier-3).

Operator runbook for code-signing the `pdfluent.exe` CLI binary (and any
future MSI / MSIX installers) so Windows SmartScreen and Microsoft
Defender accept the download. As with the macOS runbook, this is
**documentation** — no real signing happens without the operator-provided
signing identity and credentials.

---

## 1. Why sign

An unsigned Windows binary downloaded via a browser triggers:

> **Microsoft Defender SmartScreen prevented an unrecognized app from
> starting. Running this app might put your PC at risk.**

The user must explicitly click "More info → Run anyway." Many won't.
Some enterprise endpoint policies block unsigned executables outright.

After signing with a **publicly-trusted code-signing certificate**, the
binary acquires "publisher: PDFluent" identity and gradually accumulates
SmartScreen reputation as install count grows. With an **EV (Extended
Validation) certificate**, the reputation is granted immediately —
SmartScreen accepts on the first download.

## 2. Signing options (in preference order)

### 2.1 Microsoft Trusted Signing (preferred)

Microsoft Trusted Signing (formerly Azure Code Signing) is the modern
solution: Azure-hosted KMS for the private key, signed with a Microsoft-
issued EV-equivalent certificate. **No USB token, no airgap dance, no
private key on the operator's machine.**

| step | what | one-time? |
|---|---|---|
| 1 | Sign up for Microsoft Trusted Signing at https://learn.microsoft.com/en-us/azure/trusted-signing/ . Choose the per-certificate or per-signature tier. The publishing entity must be a verifiable organisation (sole proprietor / individual is not currently eligible per Microsoft's policy as of 2026-05). | yes |
| 2 | Create an Azure resource: `az signing-account create --resource-group <rg> --name <signing-account> --location <region>` (Azure CLI ≥ 2.55) | yes |
| 3 | Create a Trusted Signing Profile (one per "publisher": e.g. `PDFluent`) and an associated Identity Validation. | yes |
| 4 | Add the signing app's Azure AD service principal as a Code Signer on the profile. | yes |
| 5 | On the signing host: install the Trusted Signing client (`dotnet tool install --global Microsoft.Trusted.Signing.Client`). Authenticate via Azure CLI or service principal credentials. | each host |
| 6 | Sign a binary: `dotnet sign code azure-trusted-signing --description "PDFluent CLI" --description-url "https://pdfluent.com" --file-digest sha256 --timestamp-rfc3161 http://timestamp.acs.microsoft.com --timestamp-digest sha256 --trusted-signing-account <account> --certificate-profile <profile> dist/x86_64-pc-windows-gnu/pdfluent.exe` | per release |

**Why preferred:** the EV-equivalent reputation is immediate (no
"unknown publisher" SmartScreen warning), the private key never leaves
Azure HSM, and there is no hardware token to physically share between
operators.

### 2.2 Azure Artifact Signing (sibling to §2.1)

Azure DevOps has an "Artifact Signing" task that uses the same Trusted
Signing backend; if the publish runs from Azure DevOps pipelines this
is a one-task drop-in. Configuration mirrors §2.1.

### 2.3 USB-token EV / OV certificate (fallback)

Existing Sectigo / DigiCert / Globalsign EV certificate on a YubiKey or
SafeNet eToken (FIPS-mandated post-2023). Operator-machine bound; the
token must be physically present and the operator must enter a PIN per
signature (or per session, depending on token).

| step | what |
|---|---|
| 1 | Insert the token. Verify the device shows up: on Windows `certutil -store -user My`; on macOS/Linux via `pkcs11-tool --list-slots`. |
| 2 | On Windows, use the SignTool (Windows 10 SDK / Visual Studio Build Tools): `signtool sign /fd sha256 /tr http://timestamp.digicert.com /td sha256 /sha1 <thumbprint> "dist\\x86_64-pc-windows-gnu\\pdfluent.exe"` |
| 3 | On Linux/macOS, use **osslsigncode** (with PKCS#11 module pointed at the token): `osslsigncode sign -pkcs11engine /usr/lib/engines-3/pkcs11.so -pkcs11module /usr/lib/<vendor>-pkcs11.so -certs cert.pem -key "pkcs11:object=<keylabel>;type=private" -h sha256 -t http://timestamp.digicert.com -in pdfluent.exe -out pdfluent-signed.exe` |
| 4 | The token PIN is entered interactively. CI-side, this requires a `pkcs11-tool` PIN-cache wrapper or vendor-specific automation; the EV CA may not technically allow CI-side automation depending on contract terms. |

**Why fallback:** PIN-prompt friction; non-EV variants have weaker
SmartScreen reputation; physically losing the token bricks the
release pipeline until a replacement is shipped (CA-dependent,
days–weeks).

### 2.4 OV (Organisation Validation) certificate (cheapest, weakest)

A standard OV code-signing certificate (no hardware token requirement
historically, but post-2023 CABF rules also push these to HSM).
SmartScreen still warns until reputation accumulates (could be 50k+
installs). Choose only if budget rules out §2.1 and there is no existing
EV token.

## 3. Sign procedure (manual, per release; assumes §2.1)

```bash
# On a host with Azure CLI authenticated + Trusted Signing client installed.
BIN="dist/x86_64-pc-windows-gnu/pdfluent.exe"
TS_ACCOUNT="<your-signing-account>"
TS_PROFILE="<your-certificate-profile>"

dotnet sign code azure-trusted-signing \
  --description     "PDFluent CLI" \
  --description-url "https://pdfluent.com" \
  --file-digest     sha256 \
  --timestamp-rfc3161 http://timestamp.acs.microsoft.com \
  --timestamp-digest sha256 \
  --trusted-signing-account "$TS_ACCOUNT" \
  --certificate-profile "$TS_PROFILE" \
  "$BIN"

# Verify the signature.
osslsigncode verify "$BIN"
# Or on Windows:
#   signtool verify /pa /v "$BIN"
```

Save:
- `osslsigncode verify "$BIN"` output (issuer, timestamp, sha256)
- Microsoft Trusted Signing transaction ID

Both go into the prepublish audit report for the Windows binary release
(`benchmarks/runs/prepublish_audits/pdfluent-<v>-windows.md`).

## 4. CI integration runbook

CI signing requires Azure authentication. Two patterns:

### Pattern A — service principal (recommended)

```yaml
# .gitlab-ci.yml fragment
sign:windows-cli:
  stage: package_manual
  tags: [pdfluent-vps, pdfluent-rust]
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
      when: manual
    - if: '$CI_COMMIT_TAG'
      when: manual
  variables:
    AZURE_TENANT_ID:     $AZ_TENANT_ID       # GitLab CI/CD variable, masked
    AZURE_CLIENT_ID:     $AZ_CLIENT_ID       # GitLab CI/CD variable, masked
    AZURE_CLIENT_SECRET: $AZ_CLIENT_SECRET   # GitLab CI/CD variable, masked + protected
    TS_ACCOUNT:          $TS_ACCOUNT
    TS_PROFILE:          $TS_PROFILE
  needs:
    - job: package:cli-cross-platform
      artifacts: true
      parallel:
        matrix:
          - TARGET: x86_64-pc-windows-gnu
  script:
    - az login --service-principal -u "$AZURE_CLIENT_ID" -p "$AZURE_CLIENT_SECRET" --tenant "$AZURE_TENANT_ID"
    - dotnet sign code azure-trusted-signing
        --description "PDFluent CLI"
        --description-url "https://pdfluent.com"
        --file-digest sha256
        --timestamp-rfc3161 http://timestamp.acs.microsoft.com
        --timestamp-digest sha256
        --trusted-signing-account "$TS_ACCOUNT"
        --certificate-profile "$TS_PROFILE"
        dist/x86_64-pc-windows-gnu/pdfluent.exe
    - osslsigncode verify dist/x86_64-pc-windows-gnu/pdfluent.exe
  artifacts:
    paths:
      - dist/x86_64-pc-windows-gnu/pdfluent.exe
    expire_in: 1 month
```

Pre-conditions before this job is added to the pipeline:

- `AZ_TENANT_ID`, `AZ_CLIENT_ID`, `AZ_CLIENT_SECRET`, `TS_ACCOUNT`,
  `TS_PROFILE` set as **masked + protected** CI/CD variables in
  GitLab → project → Settings → CI/CD → Variables.
- The service principal has the **Trusted Signing Certificate Profile
  Signer** role on the profile.
- `dotnet-sdk` 8+ and the Trusted Signing client are installed in the
  runner image (one-time `apt install dotnet-sdk-8.0 ; dotnet tool
  install --global Microsoft.Trusted.Signing.Client` on
  `pdfluent-vps-ci`).

### Pattern B — USB-token (operator-machine; CI-incompatible)

If the project is forced to §2.3, signing happens on a dedicated operator
machine, NOT on CI. The signed binary is uploaded back into a CI artifact
slot via a manual GitLab Releases upload. There is no automation path.

## 5. Stop-the-line conditions

- `osslsigncode verify` (or `signtool verify /pa`) reports any mismatch.
- Timestamp server unreachable: re-run with a fallback (`http://timestamp.digicert.com`).
- Microsoft Trusted Signing rejects the submission: check the Azure
  portal "Trusted Signing → Account → Activity log" for the rejection
  reason.
- SmartScreen still warns after a §2.1 sign: confirm the certificate
  profile is EV-tier; OV-tier profiles still accumulate reputation.

## 6. Verification at any later time

```bash
# Linux (no native Windows tooling required):
osslsigncode verify pdfluent.exe
# Expect: "Signature Index: 0 (Primary Signature)" with non-empty
# "Signer's Certificate" + valid timestamp + "OK" verdict.

# Windows:
signtool verify /pa /v pdfluent.exe
# Expect: "Successfully verified" with the publisher = "PDFluent" line.
```

These commands are the definitive third-party verification of the
published Windows binary.

## 7. Cross-references

- `docs/release/SIGNING_MACOS.md` — the equivalent runbook for macOS.
- `docs/release/checklists/binary_release.md` — channel-level checklist.
- `docs/release/sha_ledger/README.md` — every signed Windows binary
  MUST be ledger-recorded (R6-1).
- Microsoft Trusted Signing docs: https://learn.microsoft.com/en-us/azure/trusted-signing/
- osslsigncode: https://github.com/mtrojnar/osslsigncode (for non-Windows hosts).
