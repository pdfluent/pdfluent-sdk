# PDFluent CLI — Enterprise Inventory (Phase 0)

- Date: 2026-05-22 · Base: `origin/enterprise/ga-hardening` @ `32e7c4ed9`

## Crates / binaries
| Crate | Binary | publish | SDK dep |
|---|---|---|---|
| `crates/pdfluent-cli` (public) | `pdfluent-cli` | `false` | `pdfluent` `=1.0.0-beta.8` |
| `crates/xfa-cli` (internal) | `pdfluent` (+ `xfa-collector`, `edge-case-report`, `accuracy-report`, `edge-case-analyzer`, `xfa-license-tool`, `corpus-render`, `font-cache-builder`) | n/a | — |

No binary-name collision (`pdfluent-cli` vs `pdfluent`).

## Source
`src/main.rs` (333 LOC: Cli/Commands/XfaCommands + 7 command fns), `src/output.rs`
(85 LOC: stable `CLI_VERSION`, `exit` codes, `exit_for`, `success`/`error` envelopes).
Clap derive; `clap_complete` for completions; `serde_json` for JSON.

## Commands (status + SDK backing)
| Command | Tier | Real SDK call |
|---|---|---|
| `info` / `inspect` | production-core | `PdfDocument::open`, `page_count`, `version` |
| `extract-text` (`--out`, `--json`, inline cap 64 KiB) | production-core | `doc.extract_text()` |
| `validate` | production-core (parse+page-count, NOT PDF/A) | open + `page_count` |
| `doctor` (`--json`) | production-core | build/env info only |
| `completions` | production-core | `clap_complete::generate` |
| `xfa flatten` | **experimental stub** | none → `NOT_IMPLEMENTED`; fresh-merge needs `--experimental` |

## Tests (`tests/cli.rs`)
27 integration tests (was 22 + 5 added this milestone): help/version, doctor (human+JSON),
internal-commands-hidden, XFA caveat, unknown-command, fresh-merge opt-in, info/inspect
(human+JSON), malformed (exit 4) + missing (exit 3) for info/extract-text/validate,
`extract-text --out` round-trip + **content==stdout**, validate no-overclaim, xfa stub exit 6,
no-secrets-in-doctor, **completions all-shells smoke**, **valid-JSON parse of all `--json`
surfaces**, **valid JSON error envelope**, **missing-required-arg = exit 2**.

## Docs / examples / completions / release
- `docs/en/cli.md` — command table, exit codes, JSON envelope, XFA caveat, license; **extended
  this milestone** with completion install, CI/automation examples, troubleshooting,
  limitations, binary coexistence.
- Shell completions: runtime command (bash/zsh/fish/powershell) — verified emit + exit 0.
- Release/packaging: `publish = false`; no release artifacts/signing/SBOM yet (planned, M5).

## CI / interaction with xfa-cli
- pdfluent-cli is a normal workspace crate (built/tested by the workspace gates).
- xfa-cli's `pdfluent` binary is consumed by XFA runners + D13/QM tooling via
  `target/release/pdfluent`; the public CLI does not touch that path. No conflict.
