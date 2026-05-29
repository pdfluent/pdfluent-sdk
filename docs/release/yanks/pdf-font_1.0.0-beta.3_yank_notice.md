# Yank notice — `pdf-font 1.0.0-beta.3`

**Yanked at:** 2026-05-29 (via `cargo yank --version 1.0.0-beta.3 pdf-font`)
**Yank acknowledged on crates.io:** API now reports `yanked: true`.
**Original publish date on crates.io:** 2026-05-03T05:56:16.631807Z (pre-dates the per-channel SHA ledger).

## Why this version was yanked

`pdf-font 1.0.0-beta.3` was published with `license-file = "LICENSE"`
referring to the PDFluent Commercial License text. The crate's source
code is, however, a derivative of the upstream `LaurenzV/hayro` project:

- `crates/pdf-font/src/lib.rs` opens with the comment
  > "This crate merges functionality from hayro-font, hayro-cmap, and hayro-postscript."
- `crates/pdf-font/Cargo.toml` lists Laurenz Stampfl as a co-author.
- Multiple files under `src/postscript/` carry
  `// Keep in sync with hayro-syntax/...` comments.

Under the upstream Apache-2.0/MIT terms, a downstream fork inherits
those licence obligations and CANNOT be unilaterally relabelled under
a proprietary licence. Publishing 1.0.0-beta.3 as PDFluent Commercial
therefore misrepresented the licence — a real legal and consumer-trust
problem.

## Remediation

1. **Yank** (this notice) — version 1.0.0-beta.3 is now `yanked = true`
   on crates.io. cargo will no longer select it for new dependency
   resolutions. Existing `Cargo.lock` files that already reference it
   continue to work, but consumers will receive a warning.

2. **License system** — a canonical license registry was introduced
   in the same commit window:
   - `docs/release/canonical_licenses.toml` — single source of truth
   - `scripts/release/license_registry_check.py` — enforcement gate
   - wired into `scripts/ci/local_ci_gate.sh` (5th gate) so the
     pre-push hook and GitLab pipeline can no longer let a mismatched
     license through.

3. **Source fix** — `crates/pdf-font/Cargo.toml` now declares
   `license = "Apache-2.0 OR MIT"`. The PDFluent Commercial `LICENSE`
   file was removed; canonical `LICENSE-APACHE` and `LICENSE-MIT`
   files (copies of the hayro-ccitt versions, carrying "Copyright (c)
   The Hayro Authors") were added.

4. **Future publish** — when the publish chain reaches `pdf-font`,
   it will publish as **`1.0.0-beta.4`** (or later) with the correct
   `Apache-2.0 OR MIT` licence. The bump (from beta.3) is required
   because crates.io is append-only — we cannot replace beta.3 in-place.

## Downstream impact

There are no known downstream consumers of `pdf-font 1.0.0-beta.3` at
this time (PDFluent's own workspace pinned via path-deps, not the
registry copy). External users that had already resolved beta.3 in
their lockfile will see a `cargo update` warning the next time they
update; they should switch to beta.4 once it is published.

## Related crates audited as part of this remediation

`pdf-render` had the same mismatch (PDFluent Commercial declared,
hayro-derivative code). It was NOT yet on crates.io at the new version,
so no yank was needed; the in-tree fix (`license = "Apache-2.0 OR MIT"`
+ dual LICENSE files) is sufficient.

The other hayro-derivative crates (`pdfluent-ccitt`, `pdfluent-jbig2`,
`pdfluent-jpeg2000`, `pdf-syntax`, `pdf-interpret`, `pdfluent-cff`,
`pdfluent-lopdf`) were already correctly declared as Apache-2.0/MIT or
MIT and required no change.
