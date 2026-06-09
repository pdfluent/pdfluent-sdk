# RFC 0002 — Single-loader audit & consolidation

**Status:** Audit + design (Phase A, milestone A4). **No code consolidation is
proposed for 1.x.** This RFC documents the pd-syntax/lopdf duality, its costs,
and three consolidation options, and recommends one.

## 1. Background: the dual-stack design

PDFluent loads every document through **two** independent PDF object models:

- **`pdf-syntax`** — a zero-copy, **read-only** parser (`crates/pdf-syntax/src/lib.rs:64`:
  *"This crate is for read-only processing, you cannot directly use it to
  manipulate PDF files."*). It backs all read operations: rendering, text and
  image extraction, page geometry, compliance inspection, and structural
  recovery (xref / page-tree rebuild). Reached via `pdf-engine`.
- **`lopdf`** (vendored as `pdfluent-lopdf`) — a mutable, in-memory `Document`
  DOM that supports **serialization**. It backs all write operations.

`ARCHITECTURE.md:119` states the rationale: read-only ops use the zero-copy
parser; write ops go through lopdf which supports full serialization.

Opening a document constructs both (`pdfluent::PdfDocument` holds `engine`
*and* `lopdf`). If either loader rejects the bytes, open fails — so a
broken-xref document that pd-syntax recovers but lopdf rejects cannot open via
the facade today.

## 2. Audit

### 2.1 lopdf footprint (workspace `src/`, this ssot)

| Crate | lopdf refs | Role |
|---|---:|---|
| pdf-manip | 429 | page/object manipulation (merge, split, rotate, image insert) |
| pdfluent | 169 | facade mutation orchestration + incremental save |
| pdf-forms | 83 | AcroForm field write + flatten |
| pdf-annot | 39 | annotation create + flatten |
| pdf-extract | 36 | text/object extraction over the lopdf DOM |
| pdf-compliance | 32 | PDF/A conversion writes |
| pdf-redact | 30 | content-stream + dictionary mutation |
| pdf-engine | 9 | bridging / interop |
| pdf-sign | 8 | DSS / LTV embedding |
| **Total** | **≈1521** | |

lopdf is **deeply load-bearing for the entire write side** of the SDK. This was
independently re-verified (the claim "lopdf cannot be cheaply removed" is
**confirmed**).

### 2.2 Read path (pd-syntax, via pd-engine)

`PdfDocument::{render_page, extract_text, extract_text_blocks, page geometry,
metadata read, compliance inspection}` resolve against the pd-syntax engine.
`load_recovery()` (xref/page-tree rebuild flags) is a pd-syntax signal.

### 2.3 Mutation path (lopdf) and the `refresh_from_lopdf` pattern

Every mutation mutates the `lopdf::Document` (`get_object_mut`, `add_object`,
`set_object`, `trailer.set`, `dictionary!`), then calls
**`refresh_from_lopdf`** (`document.rs:1637`, invoked from **7 sites**:
1276, 1314, 1380, 1490, 1609, 1631, 2005). That function:

```text
serialize lopdf clone -> bytes  (lopdf::Document::save_to)
re-parse via from_bytes_with(bytes)   // reloads BOTH pd-syntax engine AND lopdf
preserve license / limits / original_bytes
*self = reloaded document
```

So **each mutation pays a full serialize → re-parse → reload of both models**.
This is the duality's principal runtime cost and its central correctness risk:
the two representations must agree after every mutation.

### 2.4 Incremental save

`to_incremental_bytes` (`document.rs:1938`) uses
`lopdf::IncrementalDocument::create_from` (`:1961`) against the preserved
`original_bytes`. **There is no pd-syntax equivalent**; incremental (and thus
signature-preserving) save is lopdf-only. On the WebAssembly target incremental
save is an unsupported operation (`xfa-wasm` exposes typed "unsupported"
codes), so WASM is unaffected by consolidation here.

### 2.5 Recovery / reporting asymmetry

`load_recovery()` is read **once, at open** (`document.rs:300`). After the first
mutation, `refresh_from_lopdf` re-parses **lopdf-re-serialized bytes**, which
are well-formed — so `xref_rebuilt` / `page_tree_rebuilt` reset to `false`. The
`XREF_REBUILT` / `PAGE_TREE_REBUILT` diagnostics therefore describe **the
original load only**; they are silently dropped across a mutate→save cycle.
This asymmetry is inherent to the dual model and must be preserved or explicitly
addressed by any consolidation.

### 2.6 Binding implications

Bindings (Node, WASM, Python, Java, C-ABI) consume the **facade** API
(`PdfDocument` + the `TextSpanInfo` read wire form), never the loaders directly.
Consolidation must keep the facade method signatures stable; the loader choice
is an internal detail. WASM already excludes incremental save.

## 3. Options

### Option A — Add a writer path to pd-syntax (retire lopdf)
Build serialization + a mutation DOM in pd-syntax; migrate all write sites off
lopdf.
- **Pros:** one model; zero serialize/re-parse penalty; recovery state survives
  mutation; smallest long-term surface.
- **Cons:** ~8–12 weeks; must re-implement a serializer + incremental update +
  the ~1521 lopdf call sites; high regression risk across the entire write side;
  pd-syntax's public surface grows (breaking for its read-only consumers unless
  carefully gated).

### Option B — Add a read/recovery path to lopdf (retire pd-syntax read)
Give lopdf a read-only view + xref/page-tree recovery; migrate rendering/extract
off pd-syntax.
- **Pros:** one model; recovery available on the write model too.
- **Cons:** ~4–6 weeks; loses pd-syntax's zero-copy read performance and its
  proven recovery/rendering fidelity; rendering quality regression risk is high
  (rendering is our headline quality claim); large rewrite of the read side.

### Option C — Keep the dual loader in 1.x; harden the seam; plan removal in 2.0
Centralize the `refresh_from_lopdf` seam, add strict sync/determinism tests that
fail loudly if the two models diverge, and document the deprecation path.
- **Pros:** ~2–4 weeks; **no behavioural change at 1.0**; preserves both
  zero-copy read performance and lopdf's mature write/serialization;
  de-risks the imminent release; buys time to design A properly.
- **Cons:** the duality (and its per-mutation cost + recovery asymmetry) persists
  through 1.x; every new mutation feature must respect the seam.

## 4. Recommendation — **Option C for 1.x, with a committed path to Option A post-1.0**

Evidence:
- **Imminence + risk:** lopdf underpins ~1521 sites and *all* write features
  incl. incremental/signature-preserving save (no pd-syntax equivalent). A or B
  is a multi-month rewrite touching the riskiest surfaces right before release.
- **No editor dependency:** the editor consumes only the read path
  (`render_page` / `extract_text*` / `TextSpanInfo`) — consolidation is not on
  its critical path, so deferring it does not block the editor.
- **Quality preservation:** Option B risks our headline rendering fidelity;
  Option A risks the entire write side. Option C changes nothing behavioural.
- **Long-term direction:** Option A is the right end state (one model, no
  per-mutation re-parse, recovery survives mutation). It should be designed
  post-1.0 as its own RFC, *gated* by the Option-C sync tests landing first so
  the migration has a divergence safety net.

**Guardrail:** no new mutation feature should deepen the duality without
respecting the centralized seam + sync tests below. The consolidation window
narrows as more write features land.

## 5. Bounded sync test (landed with this RFC)

Per Option C, this milestone adds **one** bounded, directly-useful invariant
test (`crates/pdfluent/tests/loader_sync.rs`): after a facade mutation +
`refresh_from_lopdf`, the pd-syntax read view and the lopdf write view must
still agree on page count, and the document must remain re-openable. This codifies
the dual-loader sync invariant and will fail loudly if a future change desyncs
the models. Broader determinism is already covered by `tests/determinism.rs`
and `tests/lifecycle.rs`.

## 6. Out of scope (deferred to a future RFC)

The Option-A writer implementation, recovery-state propagation across mutation
(§2.5), and migrating the ~1521 lopdf sites. None are attempted here.
