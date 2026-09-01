# pdfluent-snippet-extract

Extracts Rust code snippets from pdfluent.com how-to pages into
testable `.rs` artefacts.

**Position in the drift-guard pipeline (Slag 2):**

```
(this tool, #1236) → tests/web_examples/*.rs → cargo test (#1237) → CI diff (#1238) → e2e parity (#1246)
```

## Quick start

```bash
# 1. Populate the cache (manual for now; `--online` coming with #1238 CI wiring).
mkdir -p tools/pdfluent-snippet-extract/cache
curl -s https://pdfluent.com/how-to/encrypt-pdf-rust \
     -o tools/pdfluent-snippet-extract/cache/encrypt_pdf_rust.html

# 2. Extract.
cargo run -p pdfluent-snippet-extract -- \
    --manifest  tools/pdfluent-snippet-extract/manifest.toml \
    --cache-dir tools/pdfluent-snippet-extract/cache \
    --out-dir   crates/pdfluent/tests/web_examples \
    --dry-run

# 3. Drop --dry-run to write.
```

## Operating modes

| Mode | How | When |
|---|---|---|
| **Offline** (default) | Read from `--cache-dir` | CI, drift-guard, local re-runs |
| **Online** (`--online`, requires `--features online`) | Fetch fresh HTML via `ureq` into the cache, then extract | Manual refresh before a drift-guard update |

CI is always offline + deterministic. The `online` mode exists so a
maintainer can bulk-refresh the cache before running the diff.

## Manifest

`manifest.toml` is a list of pages:

```toml
[[page]]
url = "https://pdfluent.com/how-to/fill-pdf-form-rust"
slug = "fill_pdf_form_rust"
# optional:
# cache_file = "fill_form.html"   # default: <slug>.html
# scope = "article"               # CSS selector narrowing the search
# pick = "first"                  # "first" (default) or "longest"
```

Each page becomes `<out-dir>/<slug>.rs`.

## Output shape

Every file carries this header and the extracted block verbatim:

```rust
//! web_examples/<slug>
//!
//! Source: <URL> (fetched YYYY-MM-DD)
//!
//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).
//! Do not edit by hand — re-run the extractor instead.

use pdfluent::prelude::*;
...
```

The header is **stable** — byte-identical across runs for the same
input. That's the property the #1238 drift-guard relies on.

## What the extractor does

1. Parses the cached HTML with `scraper`.
2. Applies the per-page `scope` CSS selector (default: whole body).
3. Looks for Rust code blocks via a prioritised list of CSS patterns
   that cover Hugo, Docusaurus, Markdoc, and Prism highlighting.
4. Rejects blocks that don't look like Rust (`cargo`, `npm`, TOML
   fragments, plain prose mistakenly tagged).
5. Picks the first (default) or longest (optional) Rust block.
6. Decodes common HTML entities (`&lt;`, `&gt;`, `&quot;`, `&#39;`,
   `&amp;`, `&nbsp;`).
7. Trims trailing whitespace per line, ensures a single trailing
   newline.
8. Writes only if the output changed — unchanged files keep their
   mtime so `cargo test` stays incremental.

## Tests

- Unit tests in `src/main.rs` cover the pickers, the Rust-ness
  heuristic, normalisation, date formatting, and header stability.
- `tests/roundtrip.rs` runs the compiled binary against a synthetic
  HTML fixture end-to-end: correct block picked, entities decoded,
  dry-run respected, idempotent re-runs don't rewrite.

```bash
cargo test -p pdfluent-snippet-extract
```

## CI drift-guard (#1238)

The `docs drift (advisory)` step of
`.github/workflows/ci-ephemeral.yml` runs the extractor in offline
mode and reports when its output diverges from the committed
`tests/web_examples/*.rs`. It is advisory until #1488.

Until 31-08-2026 this pointed at `docs-drift-guard.yml`, which had
been unable to start since 28-08 and whose work was believed to have
moved to that step -- while the step only built the extractor and
never ran it. Both halves are fixed and the duplicate is gone (#290).

Two contract files the workflow depends on:

- `tools/pdfluent-snippet-extract/cache/*.html` — the HTML snapshots
  the extractor consumes. Committed to the repo so CI never hits
  the network.
- `tools/pdfluent-snippet-extract/cache/.last-fetched` — a single
  line holding the ISO date (`YYYY-MM-DD`) that the workflow feeds
  into `--fetched`. Must match the date stamp in each generated
  header so the diff stays clean.

### Refreshing the cache

```bash
# 1. Fetch fresh HTML (requires network + the `online` feature).
cargo run -p pdfluent-snippet-extract --features online -- \
    --online \
    --manifest  tools/pdfluent-snippet-extract/manifest.toml \
    --cache-dir tools/pdfluent-snippet-extract/cache \
    --out-dir   crates/pdfluent/tests/web_examples \
    --fetched   $(date -u +%Y-%m-%d)

# 2. Update the stamp file.
date -u +%Y-%m-%d > tools/pdfluent-snippet-extract/cache/.last-fetched

# 3. Commit cache/ + the regenerated tests/web_examples/*.rs together.
```

If the cache directory is empty at CI time, the workflow logs a
notice and exits 0 — there's nothing to diff. This is the current
bootstrap state until the first online refresh lands.

## Related issues

- **#1237** — imports each `tests/web_examples/*.rs` into the
  `cargo test -p pdfluent` sweep. Done (PR #1276).
- **#1246** — end-to-end parity runner compiles + runs each
  snippet against shared fixtures.

## Non-goals

- Not a documentation generator. We only extract what the website
  publishes; we don't author canonical snippets here.
- Not translation-aware. English-only. Translation handoff happens
  later once the English set is stable.
- Not a sitemap crawler. The manifest is explicit. Discovery + auto-
  add would mask silent additions; drift-guard needs explicit
  per-page consent.
