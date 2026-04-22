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

## Follow-ups

- **#1237** — import each `tests/web_examples/*.rs` into a
  `web_examples.rs` index so `cargo test -p pdfluent` sweeps them.
- **#1238** — GitHub Action runs this extractor and fails if its
  output differs from what's committed.
- **#1246** — end-to-end parity runner compiles + runs each
  extracted snippet against shared fixtures.

## Non-goals

- Not a documentation generator. We only extract what the website
  publishes; we don't author canonical snippets here.
- Not translation-aware. English-only. Translation handoff happens
  later once the English set is stable.
- Not a sitemap crawler. The manifest is explicit. Discovery + auto-
  add would mask silent additions; drift-guard needs explicit
  per-page consent.
