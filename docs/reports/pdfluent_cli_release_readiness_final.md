# PDFluent CLI — Release & Distribution Readiness (Final)

- Date: 2026-05-24 · Branch: `quality/pdfluent-cli-release-distribution-readiness`
- Base: `origin/enterprise/ga-hardening` @ `e6f8e4e33`
- Scope honored: no publish, no binary rename, no XFA/parser/FreshMerge change, no corpus access.

## VERDICT: `PDFLUENT_CLI_OPERATIONALLY_READY_BUT_DISTRIBUTION_INCOMPLETE`

The CLI is operationally enterprise-ready: reproducible no-publish packaging + checksum +
manifest tooling, a fail-closed release gate, 30 passing tests (incl. unicode/space paths,
overwrite semantics, stdout/stderr separation, JSON validity), and complete ops/packaging/
takeover docs. **Distribution is intentionally incomplete** because actual publishing,
signing/notarization, a vendored CycloneDX SBOM step, and the OS-package-manager channels
require **business decisions** and are explicitly out of scope (no publish).

## Implemented (this milestone)
- **Packaging** `scripts/release/pdfluent_cli_package.sh`: release build → staged
  `pdfluent-cli-<ver>-<triple>/` (binary + LICENSE + CLI.md + 4 shell completions),
  `SHA256SUMS` (self-verified), `release_manifest.json` (sha256/size/version/triple/publish:false),
  opt-in tar.gz/zip + `.sha256`. Reproducible-build notes (remap-path-prefix + SOURCE_DATE_EPOCH).
- **Release gate** `scripts/release/pdfluent_cli_release_gate.sh`: fail-closed checks — publish=false,
  binary name unchanged, version consistency (Cargo==source==`--version`), LICENSE in crate+package,
  SHA256SUMS verify, no secret-assignments / private-paths / raw-artifacts/debug-symbols in package.
- **Contract tests (+3, 27→30):** unicode+space path; `--out` overwrite; error human→stderr/stdout-clean.
  (Earlier this CLI line: completions smoke, JSON validity, error-envelope, `--out` round-trip.)
- **Docs:** packaging, release checklist/runbook, binary-coexistence/takeover strategy (with live
  internal-consumer list of `target/release/pdfluent`), this final report; `/dist/` gitignored.

## Validations performed (all green, local)
`cargo fmt --all --check` · `clippy -p pdfluent-cli --all-targets -D warnings` ·
`cargo test -p pdfluent-cli` (30) · `pdfluent_cli_package.sh` → OK · release gate (staged + source) → PASS ·
SHA256SUMS self-verify · `check_no_private_paths` · no artifacts staged. publish=false; binary=pdfluent-cli.

## Unresolved — separated by class
**Business decisions (not engineering blockers):**
- Whether/when to publish (crates.io / npm / PyPI / Homebrew / Scoop / winget / deb / rpm).
- Code-signing / notarization identity; SBOM tool vendoring + attestation policy.
- Public `pdfluent` binary name (the takeover).

**Future takeover work (separate milestone):**
- Rename internal `xfa-cli` binary, repoint the documented `target/release/pdfluent` consumers,
  align clap program name `pdfluent`→`pdfluent-cli` (or to `pdfluent` at takeover), transition window.

**Real engineering blockers before actual public publish:**
- None blocking *readiness*. Before publish: run a CycloneDX SBOM step (tool not vendored here),
  and a cross-platform (Linux/Windows) build+gate run in CI (this run validated host triple only).
- Broken-pipe under large output: observed no panic on small output, but not stress-verified on a
  large committed fixture (none available) — verify with a generated large-text fixture in CI.

## Required-answer summary
- Implemented: packaging+gate tooling, +3 hardening tests, full docs.
- Validations: fmt/clippy/30 tests/package dry-run/gate PASS/checksums/private-path scan.
- Remaining before publish: SBOM step, cross-platform CI build, signing — all business/CI, not code.
- No publish · no binary rename · no XFA behavior change · no secrets/private-paths/artifacts.
