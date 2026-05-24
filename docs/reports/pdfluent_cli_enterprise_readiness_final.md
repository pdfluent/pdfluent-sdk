# PDFluent CLI — Enterprise Readiness (Final)

- Date: 2026-05-22 · Branch: `quality/pdfluent-cli-enterprise-production-readiness`
- Base: `origin/enterprise/ga-hardening` @ `32e7c4ed9`
- Scope honored: no binary rename, no publish, no XFA behavior change, no FreshMerge default change, no corpus access.

## VERDICT: `PDFLUENT_CLI_PUBLIC_BETA_READY_NO_PUBLISH`

The public `pdfluent-cli` has an enterprise-quality contract (stable exit codes + JSON
envelope, no secret/path leaks, no panics, honest caveats) and 27 passing integration tests
covering the public surface. It is **public-beta-ready without publishing**. It is **not yet
"enterprise-released"** because packaging/distribution, signing/SBOM, and the `pdfluent`
binary takeover are deliberately deferred (out of scope / business decisions).

## Answers
1. **Enterprise-ready commands:** `info`/`inspect`, `extract-text`, `validate` (parse+page-count),
   `doctor`, `completions` — real, SDK-backed, exit-/JSON-contract-tested.
2. **Beta/experimental/stub:** `xfa flatten` is an experimental **stub** (`NOT_IMPLEMENTED`;
   fresh-merge requires `--experimental`; SSF default; no Adobe parity claim).
3. **Test coverage added (this milestone, +5 → 27):** completions all-shells smoke; valid-JSON
   **parse** of every `--json` surface; valid JSON **error envelope**; `extract-text --out`
   **content == stdout** round-trip; **missing-required-arg = exit 2**. (+ `serde_json` dev-dep.)
4. **Docs added:** `docs/en/cli.md` extended (completion install ×4 shells, CI/automation
   examples, troubleshooting table, limitations, binary coexistence); enterprise inventory;
   readiness roadmap (M1–M7); this final report.
5. **Release gates now:** workspace fmt/clippy(-D warnings)/test; CLI exit-/JSON-contract tests;
   no-secrets test; private-path scan; `publish = false`; license-file present; no network/telemetry.
6. **Remains before public release:** packaging/distribution (crates.io publish flip + wrappers +
   OS package managers), signing/notarization, SBOM/checksums, versioning policy, optional
   `--quiet`/`--verbose`. (M5/M6.)
7. **Remains before `pdfluent` binary takeover:** align program name to the binary, then repoint
   every internal consumer of `target/release/pdfluent` (XFA runners, D13/QM, CI). (M7.) The
   current program-name↔binary mismatch (`pdfluent` vs `pdfluent-cli`) is the **top tracked item**.
8. **Conflict with internal XFA tooling?** No — distinct binary names; no shared path touched.
9. **`publish = false` preserved?** Yes.
10. **XFA behavior changes?** **No.** Only the CLI crate (tests/docs + a clap-comment) was touched.
11. **Pipelines green?** Local gates green (see Phase 3 in commit). Branch pipeline to confirm post-push.

## What was implemented vs deferred
- **Implemented:** 5 tests + dev-dep; full docs suite (cli.md ops sections, inventory, roadmap, final).
- **Deferred (documented):** binary rename/takeover, publish, packaging/signing/SBOM, `--quiet/--verbose`,
  `xfa flatten` production impl, encrypted-PDF decryption.
