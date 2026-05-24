# PDFluent CLI — Release Checklist / Runbook (no-publish gate)

Run before any release-candidate sign-off. Engineering gates are scripted; publish is a
separate business decision (see final report). **None of these publish.**

## Engineering gates (must all pass)
- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy -p pdfluent-cli --all-targets -- -D warnings`
- [ ] `cargo test -p pdfluent-cli` (30 tests)
- [ ] `scripts/release/pdfluent_cli_package.sh` → `PDFLUENT_CLI_PACKAGE: OK`
- [ ] `PDFLUENT_CLI_DIST=… scripts/release/pdfluent_cli_release_gate.sh` → `PASS`
- [ ] `scripts/check_no_private_paths.sh`
- [ ] no raw artifacts/PDF/PNG/secrets staged in git (`git status`)
- [ ] `publish = false` still set; binary `[[bin]]` name still `pdfluent-cli`
- [ ] `--version` / `CLI_VERSION` / `Cargo.toml` version all agree

## Release record (attach)
- toolchain: `rustc -vV` output; target triple
- `release_manifest.json`; `SHA256SUMS`; (release-time) CycloneDX SBOM
- git commit SHA of the source

## Known caveats to re-state in release notes
- `validate` = parse+page-count, not PDF/A. `xfa flatten` = experimental stub.
- Completions/`--version` use program name `pdfluent` (≠ binary `pdfluent-cli`) — see takeover doc.

## Rollback
No publish = nothing to yank. To withdraw a candidate: discard the `PDFLUENT_CLI_DIST` dir
(not in git) and the tag/record; no consumer impact (nothing was distributed).
