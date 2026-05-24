# PDFluent CLI — Enterprise Readiness Roadmap

Status legend: ✅ done (this/earlier milestone) · ◐ partial · ☐ deferred.
Scope guardrails: no binary rename, no publish, no XFA behavior change, no FreshMerge default change.

## M1 — Command contract & UX hardening
- ✅ stable help text; consistent exit-code contract (`output::exit`); consistent error
  envelope (`output::error`); `--json` on info/inspect/extract-text/validate/doctor;
  human default output; stdout=data / stderr=human-errors; no panics (usage→exit 2);
  `--out` overwrite = deterministic write to given path.
- ◐ `--quiet`/`--verbose` not present (low priority; JSON already gives quiet machine mode).
- ☐ program-name↔binary mismatch (`pdfluent` vs `pdfluent-cli`) — entangled with binary takeover (M7).
- Acceptance: every command exits per contract; JSON parses; no secret/path leaks. **Met.**
- Validation: `cargo test -p pdfluent-cli`; help/version smokes. Risk: low. Rollback: revert tests.

## M2 — Functional command completeness
- ✅ info/inspect/extract-text/validate/doctor/completions are real + SDK-backed; `xfa flatten`
  is an honest experimental stub. Tier policy: **production-core** (the 6) vs **experimental
  stub** (xfa flatten). No new production commands in scope this milestone.
- ☐ future: render/merge/split/form-fill (SDK has them) — product decision.

## M3 — Test matrix
- ✅ help/version, malformed, missing-file, exit-codes, JSON parse-validity, `--out` round-trip
  + content equality, completions smoke (4 shells), no-secrets, usage error. 27 tests.
- ◐ encrypted-PDF path asserted only as `INVALID_PDF` (no decryption); snapshot/golden output not added (low value vs maintenance).

## M4 — Docs & install readiness
- ✅ `docs/en/cli.md` extended: command reference, exit codes, JSON envelope, completion
  install (4 shells), CI/automation examples, troubleshooting table, limitations, XFA caveats,
  binary coexistence. Inventory + this roadmap + final report added.

## M5 — Packaging & distribution readiness  ☐ DEFERRED (no publish)
- crates.io: `publish = false` preserved; license-file present. Plan: flip publish + add
  README/keywords/categories (already set) when business-approved.
- Wrappers (npm/PyPI), Homebrew/Scoop/winget/deb/rpm, artifact naming, signing/notarization,
  SBOM/checksums, versioning policy — documented as a future packaging milestone. Not done here.

## M6 — Security / compliance / release gates
- ✅ no-secrets test; no network/telemetry; private-path scan in CI; license-file in crate.
- ☐ SBOM/checksums + reproducible-build notes — deferred to M5 packaging.

## M7 — Internal/public binary coexistence  ☐ DEFERRED
- ✅ documented (`pdfluent-cli` vs internal `pdfluent`; no collision).
- ☐ `pdfluent` takeover: inventory + repoint all `target/release/pdfluent` consumers
  (XFA runners, D13/QM, CI) — its own milestone; must not break internal tooling.

## Commit boundaries
1. tests + dev-dep (this milestone). 2. docs (cli.md extension + inventory + roadmap + final).
