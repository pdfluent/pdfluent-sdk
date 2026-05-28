# PDFluent CLI — Packaging & Distribution (no-publish)

Tooling: `scripts/release/pdfluent_cli_package.sh` (build/stage/checksum/manifest/archives)
and `scripts/release/pdfluent_cli_release_gate.sh` (release-safety gate). **No publish step.**

## Artifact naming & structure
`pdfluent-cli-<version>-<target-triple>/` containing:
```
pdfluent-cli                 # the binary (name unchanged — never `pdfluent`)
LICENSE                      # PDFluent Commercial License (required)
CLI.md                       # operator docs (docs/en/cli.md)
completions/pdfluent-cli.{bash,zsh,fish,powershell}
SHA256SUMS                   # checksums of every file (self-verified)
release_manifest.json        # version, triple, binary sha256+size, license, publish:false
```
Archives (opt-in `--archives`): `pdfluent-cli-<version>-<triple>.tar.gz` (+ `.sha256`), `.zip`.
Output dir: `PDFLUENT_CLI_DIST` (default `/tmp/pdfluent-cli-dist`) — **never committed** (`/dist/` is gitignored).

## Checksums & manifest
- `SHA256SUMS` covers all staged files; the packaging script self-verifies (`sha256sum -c`).
- `release_manifest.json` records `binary_sha256`, `binary_size_bytes`, `version`,
  `target_triple`, `license_file`, `completions`, `workspace_packages`, `publish:false`.

## Reproducible build notes
- Path remapping (`--remap-path-prefix`) is configured in `.cargo/config.toml` → no developer
  paths embedded in artifacts.
- `SOURCE_DATE_EPOCH` is exported (default `0`) to pin any timestamping.
- Determinism caveat: cross-host/toolchain reproducibility is **bit-exact only within the same
  rustc version + target + flags**; document the exact toolchain in the release record.

## SBOM strategy
- The package emits a deterministic dependency count from `cargo metadata`.
- Full CycloneDX SBOM is a release-time step: `cargo cyclonedx -f json` (tool not vendored here;
  install at release time). Attach the SBOM + `SHA256SUMS` to the release record. Not run here
  (no network/tool install in this milestone).

## Dry-run
```bash
scripts/release/pdfluent_cli_package.sh                 # build+stage+checksums+manifest
PDFLUENT_CLI_DIST=/tmp/pdfluent-cli-dist scripts/release/pdfluent_cli_release_gate.sh   # gate
```
Validated locally: package OK, gate PASS, checksums self-verify. No publish triggered.
