# PDFluent — Release Runbook (1.0 GA)

**Audience:** the release engineer cutting a new `pdfluent`
version. **Scope:** steps for a MAJOR or MINOR release of the
`pdfluent` meta-crate. PATCH releases follow the same runbook minus
the RFC / MIGRATION steps.

---

## 0. Pre-flight checklist

Before starting a release, confirm:

- [ ] Master CI is green locally (CI billing infra is a known issue;
      local green is the merge basis).
- [ ] No open PRs are tagged `1.0-blocker` in milestone #52.
- [ ] No public method panics on its happy path (see
      [STABILITY.md §3.3](STABILITY.md#33-deferred-truth-gaps-in-10)).
- [ ] The Codex end-audit has been run and its result is
      `Codex audit complete, milestone safe to close`.
- [ ] `CHANGELOG.md` has a `[Unreleased]` section with all
      user-visible changes since the previous tag.

---

## 1. Version bump

### 1.1 Pick the next version

Follow [STABILITY.md §2](STABILITY.md#2-semver-policy):

- **MAJOR (2.0):** any removal from the Stable set, any Error code
  change, any default-feature flip, any target-triple drop.
- **MINOR (1.x+1):** new methods, new `#[non_exhaustive]` variants,
  new Cargo features, deferred-item promotions, observability
  expansions.
- **PATCH (1.x.y+1):** bug fixes only. No additions to the public
  surface.

### 1.2 Update workspace version

```bash
# crates/pdfluent/Cargo.toml
#   version = "1.0.0"   <-- change here
# Then sync any inter-workspace pins (=1.0.0-beta.X references, etc.).
```

The meta-crate uses its own `version`, not `version.workspace = true`,
so the change is local.

---

## 2. Artifact build

```bash
# Clean, release-profile build for every default-feature combo we
# publish:

cargo clean
cargo build --release -p pdfluent
cargo build --release -p pdfluent --no-default-features
cargo build --release -p pdfluent --features tracing
cargo build --release -p pdfluent --target wasm32-unknown-unknown
```

All four must complete without warnings (fmt + clippy `-D warnings`
must already be clean from pre-flight).

---

## 3. Test sweep

```bash
cargo fmt --all --check
cargo clippy -p pdfluent --all-targets -- -D warnings
cargo test  -p pdfluent
cargo test  -p pdfluent --doc
cargo test  -p pdfluent --features tracing
cargo test  -p pdfluent-snippet-extract
```

Expected: every suite green, zero failures, only the intentional
legacy `render_png_rust_runs` ignored (tracked in `e2e_parity.rs`
under LEGACY_IGNORED_WITH_COMPANION_COVERAGE).

---

## 4. Docs regen

### 4.1 Web-example drift guard

```bash
# If the website cache has been refreshed since the last release:
cargo run -p pdfluent-snippet-extract -- \
    --manifest   tools/pdfluent-snippet-extract/manifest.toml \
    --cache-dir  tools/pdfluent-snippet-extract/cache \
    --out-dir    crates/pdfluent/tests/web_examples \
    --fetched    $(cat tools/pdfluent-snippet-extract/cache/.last-fetched)

# Diff should be empty. If not, commit + restart from §0.
git diff -- crates/pdfluent/tests/web_examples/
```

### 4.2 Rustdoc sanity

```bash
cargo doc -p pdfluent --no-deps --release
# Manually scan target/doc/pdfluent/index.html for broken intra-doc
# links and truth-gap items (they should all link to STABILITY.md §3.3).
```

### 4.3 MIGRATION.md

If this release breaks anything on the Stable set (MAJOR only):

- [ ] Add a `## From 1.x → 2.0` section with concrete renames.
- [ ] List every symbol removed, renamed, or changed-shape.
- [ ] Link to the RFC-amendment PR that justified the break.

MINOR releases: add an entry under "What's new" at the top; no
rename table needed.

---

## 5. CHANGELOG

`CHANGELOG.md` format follows [Keep a Changelog](https://keepachangelog.com/):

```markdown
## [1.x.0] — YYYY-MM-DD

### Added
- Short description, issue/PR link.

### Changed
- Behaviour clarifications (not renames).

### Fixed
- Bug fixes with issue links.

### Deferred → Stable (1.x promotions)
- `PdfDocument::linearize` now wires to the real linearizer.
  Returned `Error::MissingDependency` in 1.0.

### Deprecated
- Symbols now carrying `#[deprecated(since = "1.x", note = "...")]`.
  See STABILITY.md §12 for removal timeline.
```

MAJOR releases additionally carry a `### Removed` section.

---

## 6. Tag + publish

```bash
# 6.1 Commit CHANGELOG + version bump.
git add CHANGELOG.md crates/pdfluent/Cargo.toml
git commit -m "Release pdfluent 1.x.0"

# 6.2 Tag.
VERSION=1.x.0
git tag -a "pdfluent-v$VERSION" -m "pdfluent $VERSION"

# 6.3 Push.
git push origin master
git push origin "pdfluent-v$VERSION"

# 6.4 Publish.
cargo publish -p pdfluent
```

`cargo publish` runs `cargo package --list` internally — inspect
the file list if the tag is a MAJOR to make sure nothing private
slipped in.

---

## 7. Post-publish

- [ ] Draft a GitHub release against the tag. Body = the CHANGELOG
      section for this version.
- [ ] Close the milestone on GitHub if this is the release it's
      tied to (for 1.0: milestone #52).
- [ ] Update pdfluent.com version numbers (outside this repo).
- [ ] Announce the release on the relevant channel (email list,
      Discord, or whatever the current practice is).

---

## 8. Rollback

If a critical bug is found within 48 hours:

```bash
# Yank the broken version from crates.io. Does NOT delete the code,
# just stops cargo from selecting it.
cargo yank --vers 1.x.0 pdfluent

# Immediately cut a PATCH with the fix.
```

Yanking is not a substitute for a proper fix — treat it as an
emergency brake.

---

## 9. Binding releases

The `pdf-capi` / `pdf-java` / `pdf-node` / `pdf-python` crates have
their own runbooks in their respective README files. Their version
numbers track `pdfluent` within a minor (e.g. bindings 1.0.2 may
target pdfluent 1.0.x range) but are released independently.

Always release `pdfluent` first; bindings second.

---

## 10. Notes on the CI billing state

As of master `ecd4b4189` (Slag 2 close), GitHub Actions jobs fail
to start due to a billing/payment infrastructure state on the repo.
This is **not** a code-quality gate — every PR in milestone #52 was
local-green before admin-merge. When CI is restored:

- Re-run every green-locally PR merged during the outage to confirm
  CI parity.
- The drift-guard workflow (`docs-drift-guard.yml`) will start
  firing on PRs touching `tests/web_examples/` or the extractor.
- No code change is expected; this is a retrospective audit only.
