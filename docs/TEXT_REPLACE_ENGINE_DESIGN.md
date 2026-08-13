# Layout-Aware Text Replacement Engine — Phase 1A Design

Status: Phase 1A reviewed & decided (§10); Phase 1B implemented as
`pdf_manip::text_edit`; **Phase 1C COMPLETE** (2026-08-13):
- `CommitPolicy::BestEffort` (deterministic subset, earlier-staged wins,
  iterative re-plan, prepare-then-swap intact);
- module-level convenience `text_edit::replace_text` (every found occurrence
  accounted for — non-editable matches are Failed results, never skips);
- legacy `text_replace::replace_text{,_all_pages}` migrated onto the engine
  (4 characterization pins deliberately flipped, see
  `tests/text_replace_characterization.rs` `migrated_*` tests);
- the engine is feature-independent (hand-rolled strict token codec, no
  serde requirement; serde feature only adds Serialize derives);
- SDK facade: `pdfluent::PdfDocument::{find_text, replace_text,
  replace_text_matches}` behind new `Capability::TextEdit`, **available in
  every tier including Trial** (product decision 2026-08-13): Trial-tier
  edits stamp a small "Edited with PDFluent trial - pdfluent.com" notice on
  each modified page; licensed tiers edit without the notice. Revision
  threading and engine re-sync handled internally.
Next: Phase 2 (font metrics + fit policies), bindings exposure (WASM/C/Py).
Scope: public Rust API, FFI-friendly data structures, MatchId lifecycle,
content-container/provenance model, transaction & conflict semantics,
signature policy, supported/unsupported matrix, migration path for the
legacy `replace_text()` function.

Non-goals in this phase: reflow, shrink-to-fit, spacing adjustment, Unicode
font embedding, XObject clone-on-write editing, OCR, RTL/CJK.

---

## 1. Why the current primitive cannot be the foundation

Characterization of `pdf_manip::text_replace` (see
`crates/pdf-manip/tests/text_replace_characterization.rs`):

| # | Current behaviour | Where | Why it disqualifies |
|---|---|---|---|
| C1 | All page content streams are concatenated (`editor_for_page`), and the full result is written back into the **first** stream; the page's `/Contents` array is collapsed to a single reference | `content_editor.rs:497`, `content_editor.rs:523-575` | Destroys stream boundaries and shared-stream structure. Any page sharing one of those streams (uncommon but legal) is corrupted. Diffs are maximal: every stream is rewritten even for a one-glyph edit. |
| C2 | Replaces **all** occurrences on a page; no per-occurrence selection | `replace_text` | Cannot satisfy the core product requirement (replace occurrence #2 of a repeated header). |
| C3 | Silent skips on: missing op index, primary+fallback encode failure, cross-run encode failure, per-page FontMap failure, per-page replace failure | `text_replace.rs:40,63-65,92-98,110,122,126` | Violates "no silent skips". The caller cannot distinguish "not found" from "found but failed". |
| C4 | Font fallback (Helvetica/WinAnsi injection) is applied silently | `text_replace.rs:805-819` | Visual style change without any report. |
| C5 | `TJ` kerning arrays are flattened to a single `Tj` when replacement length differs | `text_replace.rs:453-457` | Silent layout change of the *unmatched* prefix/suffix inside the same operator. |
| C6 | Text inside Form XObjects is invisible: reported as "not found" (count 0) | `extract_text_runs` walks only the page stream | Headers/footers/stamps are routinely in XObjects. "Not found" is a lie. |
| C7 | `/ActualText`, marked content and the structure tree are ignored; glyph text is changed while `/ActualText` keeps the old string | no BDC/BMC handling anywhere | Extraction-order output (and screen readers) still produce the old text — fails the "old text no longer extractable" criterion. |
| C8 | No signature / DocMDP awareness: a certified or signed document is modified in place without warning, invalidating signatures | none | Violates signature protection requirement. |
| C9 | Errors are stringly-typed (`ManipError::Other(String)`) | `error.rs` | No typed diagnostics; FFI callers cannot branch on failure kinds. |
| C10 | No transaction: each `replace_text` call parses, edits and rewrites immediately; multi-edit batches shift operator indices between calls | API shape | Overlap/conflict detection is impossible after the fact. |

The existing cross-run offset mapping (`apply_cross_run_replacement`,
`distribute_kept_text`) and the encoding layer
(`encode_text_for_font` + round-trip verification) are sound and are
**retained** as internal building blocks of the new engine.

---

## 2. Architecture: search → plan → mutate

```
TextEditSession (borrows &mut PdfDocument state, holds revision fingerprint)
  ├── scan phase        find_text(query) → Vec<TextMatch>       (read-only)
  ├── staging phase     stage_replace(&match_id, text, opts)    (no doc mutation)
  ├── validation phase  commit() step 1: revalidate every staged edit
  │                       - revision fingerprint still current?
  │                       - locator still resolves to identical source bytes?
  │                       - encodability (with declared fallback policy)
  │                       - overlap/conflict detection (same container, overlapping
  │                         operator/byte ranges)
  │                       - signature policy check (document-level, before any edit)
  ├── planning phase    commit() step 2: group staged edits per ContentContainerId,
  │                       build one EditPlan per touched container
  └── mutation phase    commit() step 3: apply plans; only touched streams are
                          re-encoded and written; untouched streams keep their
                          original bytes, object IDs and /Contents structure
                        → TextReplacementReport
```

Hard rules:

- **Only touched containers are rewritten.** A page with `/Contents [A B C]`
  where the match lives in `B` keeps `A` and `C` byte-identical and keeps the
  three-element array. Stream `B` is re-encoded in place (same object ID).
- **Atomicity:** default `CommitPolicy::AllOrNothing` — validation runs for
  *every* staged edit before *any* mutation; first failure aborts with a
  typed error carrying the per-edit results. `CommitPolicy::BestEffort` is an
  explicit opt-in that applies the valid subset and reports every failure —
  never a silent skip: `matches_found == replacements_applied +
  replacements_failed` always holds.
- **Scan is container-aware:** the scanner descends into Form XObjects
  (recursively, cycle-guarded) so matches there are *found* and carry
  `editable = false` + `UnsupportedContainer::FormXObject` in Phase 1.

### 2.1 Content containers

```rust
/// Identifies one editable stream of content operators.
pub enum ContentContainerKind {
    /// One element of the page's /Contents (index within the array; 0 for a
    /// single-stream page).
    PageStream { index: u16 },
    /// A Form XObject reached from the page (possibly nested).
    FormXObject {
        /// Resource-name path from the page, e.g. ["Fm0", "Fm3"].
        path: Vec<String>,
        /// Number of pages referencing this XObject's object id (1 = safe to
        /// edit in place once supported; >1 = clone-on-write required).
        shared_by: u32,
    },
}

pub struct ContentContainerId {
    pub page: u32,                     // 1-based, matching pdf-manip convention
    pub stream_obj: (u32, u16),        // lopdf ObjectId of the stream
    pub kind: ContentContainerKind,
}
```

The scanner builds a `ContainerGraph` per page: the ordered list of page
streams plus every Form XObject invoked via `Do`, each parsed into its own
`ContentEditor` **without concatenation**. Cross-container matches (text
starting in stream A and continuing in stream B — possible because stream
boundaries are not token boundaries in PDF) are handled by parsing the page
streams as one *logical* operator sequence but remembering, per operation,
which source stream it came from (`Vec<(ContentContainerId, ops_range)>`).
The plan/mutate phases then write each container separately. Note the PDF
spec requirement that a content stream be a syntactically complete operator
sequence is violated by real files; if an operator straddles a stream
boundary, both streams belong to the same *fused container* and both are
rewritten (reported in the result as `containers_fused`).

### 2.2 Provenance model (internal)

Every match records, internally:

```rust
struct MatchProvenance {
    revision: RevisionFingerprint,       // see §3
    page: u32,
    container: ContentContainerId,
    ops_range: Range<usize>,             // ops in the container's editor
    /// Per boundary operator: glyph/byte sub-range inside the string operand
    /// (so an edit keeps the unmatched prefix/suffix bytes verbatim).
    start_edge: OperandSlice,            // { op_index, byte_range, glyph_range }
    end_edge: OperandSlice,
    source_bytes_hash: u64,              // xxh3 of the exact matched operand bytes
    context_hash: u64,                   // xxh3 of ±N ops window (N=4) — cheap
                                         // stale-detection without full re-scan
    style: MatchStyle,                   // font name/size, Tc/Tw/Tz/Ts, fill color,
                                         // text matrix + CTM at match start
    text: MatchText,                     // visual / actual / effective (§7)
    editable: bool,
    unsupported: Option<UnsupportedReason>,
}
```

`OperandSlice.byte_range` is the range in the *encoded string operand*;
`glyph_range` the corresponding code-unit range. Both are needed: byte
ranges drive the rewrite (splice unmatched bytes verbatim), glyph ranges
drive width measurement later (Phase 2).

---

## 3. MatchId lifecycle

```rust
/// Opaque, serializable locator for one text match.
///
/// Wire format (subject to change until 1.0; consumers must treat it as an
/// opaque token): base64url of a bincode-encoded struct
/// { version: u8, revision: RevisionFingerprint, page, container_key: u64,
///   ops_start: u32, ops_len: u16, edge offsets, source_bytes_hash }.
pub struct MatchId(String);

pub struct RevisionFingerprint(u64); // xxh3 over: file identity (/ID or byte
                                     // hash), xref generation, and an
                                     // in-session mutation counter bumped on
                                     // every committed transaction
```

Contract (Phase 1 — deliberately narrow):

1. A `MatchId` is valid **only against the exact document revision that
   produced it** (`revision` must equal the session's current fingerprint).
2. It is **serializable** (opaque string) so async workflows — find, send
   out for translation, apply days later — work, *provided the caller
   reopens the same bytes and has not committed other edits in between*.
   Reopening identical bytes yields the same fingerprint.
3. Every committed transaction bumps the revision; all outstanding
   `MatchId`s from before the commit become **stale**, including IDs whose
   container was not touched. (Coarse by design: per-container revisions
   are a possible Phase 2+ refinement, not a Phase 1 promise.)
4. **No fuzzy relocation.** A stale ID is never re-resolved by searching for
   similar text. Resolution additionally verifies `source_bytes_hash` and
   `context_hash` against the live document; any mismatch →
   `TextEditError::StaleMatch { match_id, reason }`.
5. All edits derived from one `find_text` result can be staged in one
   transaction and committed atomically.
6. A permanent ID that survives arbitrary saves and mutations is a
   **non-goal** of Phase 1.

Typed staleness reasons: `RevisionChanged`, `SourceBytesChanged`,
`ContextChanged`, `ContainerMissing`.

---

## 4. Public Rust API (Phase 1 surface)

```rust
impl PdfDocument /* pdf-engine */ {
    /// Start a text-edit session. Fails on encrypted documents whose
    /// permissions forbid content modification, per PermissionsPolicy.
    pub fn begin_text_edit(&mut self) -> Result<TextEditSession<'_>, TextEditError>;

    /// Convenience: find + stage-all + commit in one call.
    /// Equivalent to the session flow with ReplaceOptions defaults.
    pub fn replace_text(
        &mut self,
        query: TextQuery,
        replacement: &str,
        options: ReplaceOptions,
    ) -> Result<TextReplacementReport, TextEditError>;
}

pub struct TextEditSession<'d> { /* doc, fingerprint, staged edits */ }

impl TextEditSession<'_> {
    pub fn find_text(&mut self, query: TextQuery) -> Result<Vec<TextMatch>, TextEditError>;
    /// Re-hydrate a serialized MatchId (async workflows). Verifies revision
    /// + hashes; returns the full TextMatch or a typed stale error.
    pub fn resolve(&self, id: &MatchId) -> Result<TextMatch, TextEditError>;
    /// Stage a replacement. No document mutation. Cheap validation now
    /// (stale ID, duplicate staging on the same match), full validation at
    /// commit. Staging two edits on one MatchId is an immediate error.
    pub fn stage_replace(
        &mut self,
        target: &MatchId,
        replacement: &str,
        options: ReplaceOptions,
    ) -> Result<(), TextEditError>;
    pub fn staged(&self) -> &[StagedEdit];
    pub fn unstage(&mut self, target: &MatchId) -> bool;
    /// Validate everything, then apply per-container plans atomically.
    pub fn commit(self) -> Result<TextReplacementReport, CommitError>;
    /// Drop all staged edits without touching the document.
    pub fn abort(self);
}
```

### 4.1 Query

```rust
pub struct TextQuery { /* builder */ }
impl TextQuery {
    pub fn exact(text: &str) -> Self;
    pub fn case_insensitive(self, yes: bool) -> Self;   // Unicode simple fold
    pub fn pages(self, range: impl RangeBounds<u32>) -> Self;
    pub fn region(self, page: u32, rect: [f64; 4]) -> Self; // match bbox ∩ rect
    pub fn limit(self, n: usize) -> Self;               // first n matches
}
// Regex / whole-word: Phase 1B+ (builder gains variants; not in 1A contract).
```

Queries match against `effective_text` (§7) of the logical reading sequence
per container group (same grouping rule as today's cross-run matcher,
extended with same-size/style constraint, §8).

### 4.2 Match (FFI-friendly: plain data, no borrows)

```rust
pub struct TextMatch {
    pub id: MatchId,
    pub text: String,             // the matched effective text
    pub page: u32,
    pub bbox: [f64; 4],           // device space, from style.transform + widths
    pub spans: Vec<MatchSpan>,    // one per touched text-showing operator
    pub style: MatchStyle,        // font name, size, color, spacing params
    pub transform: [f64; 6],      // text matrix × CTM at match start
    pub writing_direction: WritingDirection, // Ltr only in Phase 1
    pub container: ContainerInfo, // kind, shared_by, resource path
    pub actual_text: Option<String>,   // enclosing /ActualText, if any
    pub confidence: MatchConfidence,   // Exact | DecodingAmbiguous(reason)
    pub editable: bool,
    pub unsupported: Option<UnsupportedReason>,
    pub warnings: Vec<MatchWarning>,
}
```

All API structs are `serde::Serialize/Deserialize` (behind the existing
`serde` feature) and contain only owned data — they cross WASM/C/Python
boundaries as JSON or plain structs without lifetimes. Enums are
`#[non_exhaustive]` with explicit discriminants where they cross the C ABI.

### 4.3 Options (Phase 1 subset)

```rust
pub struct ReplaceOptions {
    pub fit: FitPolicy,                  // Phase 1: only FitPolicy::Exact
    pub font_fallback: FontFallback,     // Deny (default) | Explicit(name) |
                                         //   InjectStandard — never silent:
                                         //   any fallback use lands in report
    pub commit_policy: CommitPolicy,     // AllOrNothing (default) | BestEffort
    pub signature_policy: SignaturePolicy, // §6
    pub tagged_text_policy: TaggedTextPolicy, // §7: Reject (default) in 1A
}
```

`FitPolicy::Exact` in Phase 1A/1B means: replacement is encoded in the same
font; no measurement-based failure yet (metrics land in Phase 2), but the
report always carries old/new glyph counts so callers can gate on growth.
The enum ships with all five variants (`Exact`, `AdjustSpacing`,
`ShrinkToFit`, `ReflowInBounds`, `ExpandBounds`) so FFI signatures are
stable; unimplemented variants return `TextEditError::UnsupportedFitPolicy`
— visible, not silent.

### 4.4 Report

```rust
pub struct TextReplacementReport {
    pub matches_found: usize,
    pub replacements_applied: usize,
    pub replacements_failed: usize,     // BestEffort only; 0 under AllOrNothing success
    pub pages_modified: Vec<u32>,
    pub containers_modified: Vec<ContainerInfo>,
    pub containers_fused: Vec<ContainerInfo>,   // §2.1 straddling case
    pub signatures_present: bool,
    pub signatures_invalidated: bool,           // true iff policy allowed the edit
    pub results: Vec<TextReplacementResult>,    // one per staged edit, always
}

pub struct TextReplacementResult {
    pub match_id: MatchId,
    pub status: ReplacementStatus,       // Applied | Failed(TextEditErrorKind)
    pub old_bbox: [f64; 4],
    pub new_bbox: Option<[f64; 4]>,
    pub font_used: String,
    pub font_substituted: bool,
    pub old_font_size: f64,
    pub new_font_size: f64,
    pub fit_applied: FitPolicy,
    pub new_line_count: u32,
    pub overflow: Option<OverflowInfo>,
    pub actual_text_updated: Option<bool>, // None = no ActualText present
    pub tags_affected: bool,
    pub diagnostics: Vec<Diagnostic>,    // code + message + remediation hint
}
```

### 4.5 Typed errors

```rust
#[non_exhaustive]
pub enum TextEditError {
    StaleMatch { match_id: MatchId, reason: StaleReason },
    OverlappingEdits { a: MatchId, b: MatchId },
    DuplicateStage { match_id: MatchId },
    UnsupportedContainer { match_id: MatchId, kind: UnsupportedContainer },
    UnsupportedStyleSpan { match_id: MatchId, detail: String },     // §8
    EncodingFailed { match_id: MatchId, ch: char, font: String },
    FontFallbackDenied { match_id: MatchId, ch: char },
    TaggedTextConflict { match_id: MatchId },                       // §7
    SignedDocumentRejected { signatures: Vec<SignatureSummary> },   // §6
    PermissionsDenied,
    UnsupportedFitPolicy(FitPolicy),
    Document(ManipError),
}

pub struct CommitError {
    pub error: TextEditError,           // first fatal cause
    pub results: Vec<TextReplacementResult>, // full per-edit validation outcome
}
```

---

## 5. Transaction & conflict semantics

- **Overlap detection:** two staged edits conflict when their containers are
  equal and their `(ops_range, edge byte ranges)` intersect. Adjacent edits
  inside one operator (disjoint byte ranges) are allowed and applied
  right-to-left so earlier offsets stay valid.
- **Ordering:** within a container, edits are applied in descending
  `ops_range.start` (then descending byte offset), so operator indices never
  shift under earlier staged edits. Across containers, order is irrelevant.
- **Validation before mutation:** the entire staged set is validated against
  the live document (stale, overlap, encoding, signature, permissions)
  before the first byte changes. Under `AllOrNothing` any failure aborts
  with `CommitError` carrying all per-edit results. Under `BestEffort`,
  failed edits are excluded and reported; a failure *during* mutation of a
  container rolls back that container to its pristine operator list before
  continuing (each container mutates a clone; the write happens only after
  the clone re-encodes successfully). The document object is only touched
  by successfully re-encoded containers.
- **One commit per session:** `commit(self)` consumes the session. A new
  session sees the new revision.

---

## 6. Signature & permissions policy

Checked once per commit, before any mutation, via `pdf-sign`
(`signature_fields`, DocMDP extraction):

```rust
pub enum SignaturePolicy {
    /// Default. Any signature field with a value ⇒ commit is rejected with
    /// SignedDocumentRejected listing the signatures found.
    RejectSignedDocuments,
    /// Caller explicitly accepts breaking signatures. Commit proceeds;
    /// report sets signatures_invalidated = true and lists them.
    AllowPostSignatureChange,
}
```

- DocMDP: a certification signature with `P = 1` (no changes) or `P = 2/3`
  is treated identically in Phase 1 — content-stream text replacement is a
  disallowed change under every DocMDP level, so only the policy flag
  differentiates behaviour. The DocMDP level is included in
  `SignatureSummary` for the caller's benefit.
- Documentation claim (exact wording contract): *"With
  `AllowPostSignatureChange`, PDFluent performs a full-rewrite save; the
  signed revision's bytes are not preserved and existing signatures will
  fail validation."* We do **not** claim signature-preserving editing.
  Incremental-update saves (which would preserve the signed byte range but
  still invalidate the signature's coverage semantics for most viewers) are
  a Phase 2+ save-strategy decision; nothing in this API promises them.
- Permissions: for encrypted documents, `begin_text_edit` honours the
  modify-contents permission bit (P bit 4/6 per ISO 32000) →
  `PermissionsDenied`. Owner-decrypted documents pass.

---

## 7. Tagged / marked-content text

Per match, three text values:

- `visual_text` — decoded glyph text (today's `decode_string` output);
- `actual_text` — innermost enclosing `/ActualText` (BDC property list),
  when present;
- `effective_text` — what extraction produces: `actual_text` if present,
  else `visual_text`. **Queries match against `effective_text`.**

Phase 1 policy (`TaggedTextPolicy::Reject`, the only variant implemented):

- Match entirely inside an `/ActualText` span where `visual_text` would be
  edited: replacement fails with `TaggedTextConflict` — updating glyphs
  while `/ActualText` keeps the old string would leave the old text
  extractable (acceptance-criterion violation), and rewriting the BDC
  property list is XObject-adjacent work deferred past 1A. The diagnostic
  carries the enclosing span and both strings.
- Match with no enclosing `/ActualText`: proceed. If the *page* has a
  structure tree, the result sets `tags_affected = true` (coarse Phase 1
  signal; StructTree updates are Phase 4).
- `/ToUnicode` is a font mapping, not document content: it is left intact.
  Text replacement is not secure redaction, and the docs say so explicitly.

Scanner requirement: the container parser must track BDC/BMC/EMC nesting
with property-list resolution (inline dict or /Properties resource) — this
is new; today's `extract_text_runs` ignores marked content entirely.

---

## 8. Phase 1 style scope

Editable (Phase 1):

- match inside a single style run;
- match spanning consecutive runs with **identical** font name, size, and
  text-space transform (today's cross-run grouping requires only the same
  font *name* — the new grouping adds size/transform equality);
- exact selection of one occurrence; unmatched prefix/suffix bytes inside
  boundary operators preserved verbatim (byte splice, not re-encode);
- `TJ` kerning: elements fully inside prefix/suffix are preserved verbatim,
  including their spacing adjustments (fixes C5); elements overlapping the
  match are rewritten.

Explicit `UnsupportedStyleSpan` diagnostic (not silent) for matches
spanning: different fonts, sizes, bold/italic variants, colors, or
transforms; Type3 fonts; text with `Tr 3` (invisible) gets a warning, not a
rejection. A future `ReplacementStyle` option (rich per-segment styling) is
reserved in `ReplaceOptions` naming but not designed here.

Supported / unsupported matrix (Phase 1):

| Area | Supported | Unsupported (typed diagnostic) |
|---|---|---|
| Operators | Tj, TJ, ', " | Type3 `d0/d1` content, shading text |
| Containers | page streams (multi-stream preserved) | Form XObject (detect + report, no edit), Tiling patterns, annotation appearance streams |
| Scripts | Latin LTR | RTL, CJK, vertical, complex shaping |
| Fonts | simple fonts w/ ToUnicode or Encoding; CID Identity-H w/ ToUnicode | outlined text, fonts w/o recoverable encoding (typed error instead of Latin-1 guess for subset/symbolic — unchanged rule) |
| Fit | Exact (glyph-count reporting) | AdjustSpacing, ShrinkToFit, ReflowInBounds, ExpandBounds |
| Tagged | untagged text; tagged w/o ActualText (flagged) | ActualText-covered spans (reject) |
| Signatures | detect + policy | preserving validity |
| Encryption | decrypted docs w/ modify permission | permission-restricted docs (typed error) |

---

## 9. Migration path for legacy `replace_text()`

1. **Phase 1A (this round):** no changes to `pdf_manip::text_replace`.
   Characterization tests pin current behaviour, including its defects, so
   any accidental behaviour change is caught.
2. **Phase 1B (DONE):** new engine landed as `pdf_manip::text_edit` —
   scanner (container graph, marked content), provenance, staging, commit,
   report. Legacy functions untouched; encoding helpers shared.
3. **Phase 1C (DONE):** `text_replace::replace_text{,_all_pages}`
   reimplemented as a thin wrapper: `find(exact) → stage editable →
   commit(BestEffort, AllowPostSignatureChange, InjectStandard fallback)`;
   returns the legacy `usize` (applied count). Flipped pins (each a
   `migrated_*` test): multi-stream collapse → preserved; ActualText
   desync → span left untouched; TJ flatten → operator + outside-kerning
   preserved; silent cross-run fallback skip → succeeds with fallback.
   New: encrypted documents whose permissions forbid modification are now
   refused instead of attempted.
4. **pdf-engine / bindings:** `PdfDocument::begin_text_edit` +
   `replace_text` exposed through pdf-engine first (desktop app + WASM
   `edit_handle` migrate), then pdf-capi/Python/.NET/Java with the plain
   JSON report structs.

---

## 10. Review decisions (RESOLVED 2026-08-13)

### 10.1 Revision invalidation — whole-document, accepted

Every successful commit invalidates every previously issued `MatchId`,
including IDs for untouched containers. Reopening byte-identical input
produces the same initial revision fingerprint; read-only operations do not
change the revision.

The fingerprint MUST always include a cached **cryptographic digest of the
exact input bytes (SHA-256)**. The PDF `/ID` entry is metadata only and MUST
NOT replace the byte digest (it is not guaranteed to change between
revisions). An in-session mutation counter distinguishes committed revisions
that have not yet been serialized: `RevisionFingerprint = SHA-256(input
bytes) + commit counter`. In the pdf-manip layer the counter is threaded
explicitly: `DocumentRevision::from_source_bytes(bytes)` starts at counter
0, and every report carries `next_revision` for the caller to pass into the
next `begin_text_edit`. Per-container revision tracking remains a possible
future optimization, not part of the Phase 1 contract.

### 10.2 MatchId wire format — versioned JSON in base64url

```
pdfluent-match-v1.<base64url(versioned-json-payload)>
```

Consumers MUST treat the complete token as opaque. JSON was chosen (over
bincode) for cross-language compatibility, controlled schema evolution,
diagnostics and strict validation of externally persisted IDs. Decoding MUST
enforce: recognized format version; maximum token and field lengths; valid
numeric ranges; valid document fingerprint; resolvable container and
operator provenance; matching source and context hashes. Base64url provides
transport-safe encoding, not authenticity — every locator is fully
revalidated before use. A bounded binary `v2` codec may be added later if
token size proves material; raw Rust struct layout is never a persistence
contract.

### 10.3 Region matching — positive-area intersection, with explicit relation

`region()` defaults to positive-area bounding-box intersection (tolerant of
font-metric rounding; touching an edge without positive-area overlap does
not match, subject to a small documented geometry epsilon).

```rust
pub enum RegionRelation {
    Intersects, // default
    Contained,  // complete match bbox inside the supplied region
}
```

### 10.4 BestEffort timing — deferred to Phase 1C (since implemented)

Phase 1B supports only `AllOrNothing` and first establishes correctness for
container-aware scanning, persistent provenance, stale-locator detection,
conflict detection, edit planning and document-wide atomic commit. All
touched containers are parsed, modified and re-encoded into temporary
results before any live document object is mutated; only after every staged
edit and every encoded container passed validation are the prepared objects
swapped in. `BestEffort` (deterministic valid-subset selection, container-
level failure semantics, rollback, per-edit reporting) is designed and
implemented in Phase 1C alongside the legacy convenience-wrapper migration.

### Phase 1B narrowings (implementation notes)

- **Fused containers** (operator tokenization straddling a stream boundary,
  only seen in malformed files): detected — the whole page falls back to a
  fused scan and its matches are reported with
  `UnsupportedContainer::FusedPageStreams` — but not editable in 1B.
- **Page streams shared between pages** are detected and rejected as
  `UnsupportedContainer::SharedPageStream` (editing would leak into the
  other page).
- **Matching operates on `visual_text`**; when an enclosing `/ActualText`
  is present the match carries it and is rejected at stage time with
  `TaggedTextConflict` (design §7). Matching on `effective_text` where it
  diverges from the glyph text needs glyph↔ActualText offset mapping and is
  deferred with the Phase-4 structure-tree work.
- `CommitPolicy`, `FitPolicy` and `TaggedTextPolicy` ship with their full
  variant sets; unimplemented variants fail fast at stage time with typed
  errors (`UnsupportedCommitPolicy`, `UnsupportedFitPolicy`, …).

---

## 11. Enforcement layering & corpus validation (Phase 1 close-out, 2026-08-13)

### Enforcement layering (decided)

Two product surfaces share the engine; enforcement lives at exactly one:

| Surface | Path | Enforcement |
|---|---|---|
| **SDK** (licensed) | `pdfluent::PdfDocument::{find_text, replace_text, replace_text_matches}` and the `TextEditor` handle in each binding, which owns a `pdfluent::PdfDocument` | `Capability::TextEdit` (all tiers) + trial notice on modified pages when the effective tier is Trial |
| **Free desktop editor** | `pdf_manip::text_replace` wrapper and `xfa-wasm::edit_handle` (the editor's WASM edit path) | none, deliberately — the editor is free without watermarks (product rule) |

`pdf_manip::text_edit` itself stays enforcement-free: it cannot distinguish
the free editor from an SDK consumer, and auto-stamping (as `embed_fonts`
does via `LicenseGuard::load_from_env`) would watermark editor users'
documents. Direct use of `pdf-manip` by third parties is restricted legally
(LICENSE.md §3), not technically — consistent with the crate being
source-published.

**Correction (2026-08-13).** An earlier draft of this table claimed every
binding inherits enforcement "since they all wrap the `pdfluent` crate". That
was wrong, and worth recording: pdf-capi, pdf-node and pdf-python *depend* on
the `pdfluent` crate but their existing document handles call `pdf_engine`,
`lopdf` and the engine crates directly, and **none of them checks a capability
for any operation**. Depending on the crate is not the same as going through
the facade.

Every text-edit surface therefore ships as its own `TextEditor` handle that
owns a `pdfluent::PdfDocument`, which is what actually routes it through
`require_capability` and the trial notice:

| Binding | Handle | Verified end-to-end |
|---|---|---|
| WASM | `xfa_wasm::text_edit::TextEditor` | Node, `--target web` |
| Python | `pdfluent.TextEditor` (PyO3) | arm64 venv, built wheel |
| Node | `TextEditor` (napi) | built native module |
| C ABI | `pdf_text_editor_*` + `PdfStatus::ErrorTextEdit` (21) | C program against the dylib |
| .NET | `PDFluent.TextEditor` | x86_64 dotnet against the C ABI |

Each was checked the same way: open, find one occurrence, round-trip the match
id through JSON, apply, save, then confirm in the output that the new text is
present, the old text is gone, and the trial notice is stamped (all smokes ran
unlicensed). Stale-id refusal was asserted in every binding that can express
it. The pre-existing handles in those crates are left as they are — retrofitting
capability checks onto them is a separate, breaking change.

### Corpus validation (legacy migration A/B)

`xfa-test-runner --tests text_replace` over 94 local PDFs (pdfa-holdout
workset + corpus + corpus-mini + XFA golden), old engine (master
`785b2f3a5`) vs migrated wrapper:

| | Pass | Fail | Crash | Timeout | Skip | Applicable pass rate |
|---|---|---|---|---|---|---|
| old | 70 | 0 | 1 | 3 | 20 | 94.59% |
| new | 74 | 0 | 1 | 0 | 19 | **98.67%** |

Per-file diff: 4 status changes, all improvements (1 skip→pass, 3
timeout→pass), zero regressions. The single crash is identical on both
sides and is a pre-existing `u32::pow` overflow in
`pdf_interpret::function::type0` during **extraction** (tracked separately;
not a text-edit issue).

---

## 12. Shipped (2026-08-13)

Merged to `master` (`fb770ebb6`) and pushed to both remotes (GitLab `origin`,
GitHub `github`). Local CI gate green on push: metadata, fmt, build, clippy,
licenses.

**Published:** `@pdfluent/sdk-wasm@1.0.0-beta.17.4` (npm, dist-tag `latest`),
the first release carrying `TextEditor`. Verified after publish by pulling the
tarball back from the registry: `TextEditor` present in the typings, zero
private-path strings in the shipped `.wasm`.

Functional proof on the built package (Node, `--target web`): open a PDF,
`findText` one occurrence, pass its id through a JSON round-trip as a
translation service would, `replaceMatches`, `save` — output contains the new
text, no longer contains the old text, and carries the trial notice (the smoke
ran unlicensed, i.e. Trial tier).

### Deliberately not done here

- **Website WASM pin.** `pdfluent.com` pins `@pdfluent/sdk-wasm@1.0.0-beta.11`
  (`public/wasm/sdk-wasm-manifest.json`). That lag predates this work by six
  releases. Refreshing it changes what runs for live visitors, and the website
  deploys its working tree straight to Cloudflare Pages, so the jump belongs in
  its own reviewed change (`scripts/wasm/sync-sdk-wasm-from-npm.sh` +
  `check-sdk-wasm-canonical.sh`).
- **C-ABI / Python / Node / Java / .NET surfaces.** Each wraps the `pdfluent`
  facade, so each inherits licensing and the trial notice for free by
  following the `xfa-wasm::text_edit` pattern; their publish channels are
  CI-gated and out of scope for this round. Nothing regressed for them: they
  pick up the engine through the existing `replace_text` wrapper.
- **A2 SHA-256 anchor refresh.** Gate E is informational while the local
  version differs from the ledger anchor (`1.0.0-beta.11`). Now that
  `1.0.0-beta.17.4` is published, the anchor can be refreshed to make Gate E
  strict again (GA-R1-3-α).
