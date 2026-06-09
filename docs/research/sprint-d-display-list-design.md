# Sprint D — Display-List / IR Caching: Design & Deferral

**Date:** 2026-06-08
**Branch:** `xfa/sdk-sprint-d-display-list`
**Status:** DEFERRED — precise design below. No render code changed.

## Objective

A display-list / operator-list IR for page rendering (à la MuPDF `fz_display_list`,
PDF.js operator-list replay): parse/interpret a page's content **once**, then
replay/rasterise repeatedly at different DPI / zoom / clip without
re-interpreting — speeding up repeated renders, zoom, thumbnails, and editor use.

## Answers to the "before coding" questions (evidence)

**What is the current equivalent of an operator stream?**
The `Device<'a>` trait (`crates/pdf-interpret/src/device.rs`) is the operator
interface. `interpret_page(page, ctx, device)` walks the content stream and
drives the device: `draw_path`, `draw_glyph`, `draw_image`, `push_clip_path`/
`pop_clip_path`, `push_transparency_group`/`pop_transparency_group`,
`set_soft_mask`, `set_blend_mode`, `draw_rect`, `begin/end_marked_content`. The
rasteriser (`crates/pdf-render/src/renderer.rs` `Renderer`, vello_cpu) implements
`Device` and builds a vello scene, rasterised once via `render_to_pixmap`.

**Where can we capture operations without changing semantics?**
At the `Device` boundary — a `RecordingDevice` that turns each call into an owned
op, then a replay that feeds ops to the rasteriser `Device`. This is the only
seam that does not change interpretation semantics.

**How do the resources flow today?**
Everything is borrowed with lifetime `'a` tied to `&'a XRef` (owned by `Pdf` via
`Arc`):
- `Paint<'a>` = `Color` (owned) **or** `Pattern(Box<Pattern<'a>>)` — tiling
  patterns hold `Stream<'a>` + `Resources<'a>`; shadings hold functions/mesh.
- `Glyph<'a>` = `Outline(OutlineGlyph)` (owned outline) **or**
  `Type3(Box<Type3Glyph<'a>>)` which holds `&'a XRef`, `State<'a>`,
  `Resources<'a>`.
- `Image<'a,'b>` borrows the image XObject and (for stencils) a `Paint<'a>`;
  decoded pixels come from the document-level image cache.
- `SoftMask<'a>` wraps `Arc<Repr<'a>>` holding a `FormXObject<'a>` + `&'a XRef`.
- Transforms: the `Device` receives `(object-space path/glyph, CTM affine)`
  pairs, but the CTM is **pre-composed with the DPI-dependent `initial_transform`**
  (`Context::new`, `crates/pdf-render/src/lib.rs`). DPI-independence therefore
  requires recording `initial.inverse() * received` and replaying
  `new_initial * recorded`.
- Type3 glyphs, tiling patterns, and soft masks are **recursive content
  streams** — they re-enter `interpret(...)` on a sub-device
  (`font/type3.rs`, `pattern.rs`, `soft_mask.rs`).

**Which operations are safe to cache (cheap, owned)?**
`draw_path`/`draw_rect` with `Paint::Color`, `push_clip_path`/`pop_clip_path`
(`ClipPath` is `Clone`, a `BezPath` + enum), `set_blend_mode`,
`begin/end_marked_content`, `text_adjustment`, and `draw_glyph` with
`Glyph::Outline` (capture the outline `BezPath` + transform + colour).

**Which are unsafe / must remain interpreted live?**
- `Paint::Pattern(Tiling)` — a recursive content stream borrowing `'a`.
- `Paint::Pattern(Shading)` — complex function/mesh evaluation.
- `Glyph::Type3` — holds `&'a XRef` and recursively interprets a content stream;
  cannot be stored owned without re-interpretation.
- `SoftMask` and `push_transparency_group` with a mask — recursive form-XObject
  content streams; recording means re-interpreting + storing a rasterised mask.
- `draw_image` — would duplicate large decoded pixels unless it stores the image
  **cache key** and re-fetches at replay.

**What is the cache key?**
`(document identity, page index, content-stream hash)` — the content-stream hash
detects mutation. Annotation appearance must be parameterised (recorded
separately or replayed conditionally on `render_annotations`).

**What invalidates the cache?**
Document mutation (`flatten_annotations`, redact, metadata commit) goes through
`refresh_from_lopdf`, which builds a **new** `PdfDocument` — so a cache owned by
`PdfDocument` is orphaned automatically. XFA flatten likewise opens a new doc.
Page-tree restructuring invalidates affected entries. DPI/zoom/clip do **not**
invalidate (the IR is resolution-independent).

**How is memory bounded?**
Same policy as the image cache (`crates/pdf-interpret/src/cache.rs`): per-document,
bounded by entry count **and** byte budget, LRU eviction. A complex page's op
list can be large, so the byte budget is essential.

**How does it interact with the image-decode cache?**
The display list stores image **cache keys**, not pixels; replay fetches decoded
images from the shared document `Cache` exactly as the interpreter does today —
no duplication.

**How does it interact with annotation flattening and incremental save?**
Both mutate the document and re-parse via a new `PdfDocument`, orphaning the
cache (no explicit invalidation needed). Flattening bakes annotations into page
content, so a post-flatten display list would simply record the baked content.

## Measured baseline (warm, same document, 20-render mean)

| Document | Content | DPI | ms/render |
|---|---|---|---|
| multi-page.pdf | text, 53 objects | 72 | 3.05 |
| multi-page.pdf | text, 53 objects | 150 | 8.05 |
| scanned.pdf | image | 150 | 7.97 |
| simple.pdf | near-empty | 150 | 7.88 |

At a fixed DPI the cost is ~8 ms **regardless of content complexity** (text-heavy,
image, and near-empty pages all land at ~8 ms @ 150 dpi), and it scales with
pixel count (multi-page: 3.05 ms @ 72 dpi → 8.05 ms @ 150 dpi, tracking the ~4.3×
pixel increase). **Rasterisation dominates per-render time, not interpretation** —
and a display list does **not** eliminate rasterisation (it still runs per DPI).
Combined with the fact that stream bytes and decoded images are already cached,
the interpretation savings a display list would unlock are a small fraction of
per-render time on representative documents. The large owned-resource refactor is
therefore not justified by these measurements; the win is material only for
pages with very heavy vector content (thousands of ops), which should be measured
on a heavier corpus before committing to the work.

## Verdict: DEFERRED (not bounded enough for a production-quality, byte-identical sprint)

The concept is right and the `Device` seam is the correct capture point, but the
**hayro interpret stack is built entirely on borrowed `<'a>` types tied to the
`XRef`**, and the three hardest operation classes — tiling patterns, soft
masks / transparency groups, and Type3 glyphs — are **recursive content streams
holding `&'a XRef`**. A replayable owned IR therefore requires one of:

- **(A) Rasterise-and-cache tiles** (store a base-resolution bitmap per page).
  Bounded, but **loses DPI-independence and zoom quality** — it is a render
  cache, not a display list, and does not meet the "replay at different DPI"
  objective.
- **(B) Owned-resource model** — extend the interpreter to emit *owned* draw
  primitives into the IR: outline glyphs (already owned), paints resolved to
  owned colours, **patterns expanded** and **soft masks pre-rasterised to owned
  alpha masks**, Type3 glyphs **pre-interpreted** into nested op lists. This is
  the true `fz_display_list` equivalent and is a **multi-week, fork-wide refactor**
  of `pdf-interpret` (hayro fork), with a high byte-identical-output bar across
  patterns/shadings/soft-masks/transparency/Type3.

A narrow "easy ops only + live fallback for the hard ops" version is **not**
production-quality: mixing recorded and live-interpreted operations is not
byte-identical, text-heavy pages need the glyph path (workable) but any page with
a pattern/soft-mask/Type3 falls back wholesale, and maintaining a parallel render
path is a standing correctness burden. It is therefore rejected.

What is *already* cached today also lowers the payoff: content-stream bytes are
decoded once per `Page` (`OnceLock`), and decoded images are cached
document-wide (Sprints A/B). The remaining per-render cost a display list would
remove is **re-tokenisation + re-interpretation** (resource resolution, glyph
outline extraction, soft-mask rasterisation). See the measured baseline in the
sprint report.

## Recommended phased milestone (when scheduled)

1. **Prerequisite — owned-resource model in `pdf-interpret`.** Introduce owned
   equivalents (`OwnedPaint`, `OwnedGlyph`, pre-rasterised `OwnedMask`,
   expanded-pattern ops) and a `RecordingDevice<'a>` that converts each `Device`
   call — recursively interpreting patterns/soft-masks/Type3 *at record time* —
   into a DPI-independent op list (factoring out `initial_transform`).
2. **Display-list IR core** — the owned op enum + `replay(ops, &mut dyn Device,
   new_initial_transform)`.
3. **Cache/replay integration** — a per-`PdfDocument`, count+byte-bounded LRU
   keyed by `(page index, content-stream hash)`, referencing the image cache by
   key; transparent behind `render_page` with no public API change.
4. **Validation** — record-vs-direct byte-identical render equivalence across
   corpus-mini (text, image, annotated), multi-DPI replay, clip replay, memory
   bound, and post-mutation invalidation; repeated-render benchmark.

Until the owned-resource prerequisite exists, a byte-identical general display
list cannot be built without unsafe lifetime escapes. Defer.
