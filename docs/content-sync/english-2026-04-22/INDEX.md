# English content sync — 2026-04-22

**Source of truth:** `pdfluent` SDK at master `e891ffb0d`.
**Frozen API contract:** [RFC 0001 v1.3](../../rfc/0001-sdk-core-api.md).
**Stability register:** [STABILITY.md](../../../STABILITY.md).
**Trigger:** First online drift-guard refresh (FASE C of milestone #52 closure).

## Scope

Five how-to pages on pdfluent.com whose Rust snippets don't match
the SDK they claim to demonstrate.

| Page | Status | Action required |
|---|---|---|
| [`encrypt-pdf-rust`](./encrypt_pdf_rust.md) | **WEBSITE_PROMISES_UNSUPPORTED_BEHAVIOR** | Replace snippet. API uses imaginary builder method chain. |
| [`extract-text-pdf-rust`](./extract_text_pdf_rust.md) | **NEEDS_ENGLISH_UPDATE** | Replace `page.extract_text()` with `page.text()` (the actual SDK method). |
| [`fill-pdf-form-rust`](./fill_pdf_form_rust.md) | **WEBSITE_OLDER_THAN_SDK** | Remove `?` after `form_mut()` — the accessor is infallible per RFC v1.1. |
| [`merge-pdfs-rust`](./merge_pdfs_rust.md) | **WEBSITE_PROMISES_UNSUPPORTED_BEHAVIOR** | Replace snippet. `PdfMerger::merge(path)` signature differs; field access `output.page_count` is a method call. |
| [`render-pdf-to-png-rust`](./render_pdf_to_png_rust.md) | **WEBSITE_PROMISES_UNSUPPORTED_BEHAVIOR** | Replace snippet. No per-page `page.render()`; use `PdfDocument::to_images()`. |

## How to apply

Each per-page file contains:

1. **Current published snippet** — copy-paste from the live site at fetch time (2026-04-22). Used for diff reference.
2. **Canonical SDK-truth snippet** — drop-in replacement, already compile-tested via `cargo test -p pdfluent --test web_examples` on master.
3. **Prose-change notes** — any surrounding English paragraphs whose claims are now false.
4. **What stays the same** — intro, SEO title, closing notes, etc.

## Truth-gaps to keep visible

Even after this sync, the website should continue to mark four
items as deferred per STABILITY.md §3.3:

- `linearize()` — returns `Error::MissingDependency`, 1.1 follow-up.
- `embed_font()` — same.
- `add_decoration()` / `add_watermark()` — return `Error::MissingDependency` until the watermark runtime lands (#1223).
- `flatten_forms()` — now returns `Error::MissingDependency` (no longer panics — the GA blocker fixed in PR #1280).

If any of these have dedicated how-to pages that promise runtime support, those pages also need updating — they should **explicitly** say the method returns `MissingDependency` today and point at the tracked issue.

## Follow-through

After the website edits go live:

1. Maintainer runs the drift-guard online refresh per `RELEASE_RUNBOOK.md` §4.1 to re-cache the HTML + regenerate `tests/web_examples/*.rs`.
2. CI `docs-drift-guard.yml` workflow will begin firing on meaningful cache inputs instead of vacuous-no-op.
3. Translation handoff (FASE E) produces the Minimax package against the now-stable English source.

## Who does what

- **Website code edits** — website repo maintainer or Cloud/Claude session with website-repo access.
- **Translations** — Minimax (FASE E).
- **Translation integration + deploy** — Cloud/Claude (FASE F).
