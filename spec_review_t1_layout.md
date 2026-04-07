# Spec Review T1: XFA 3.3 Chapter 8 — Layout for Growable Objects

**Crate:** `xfa-layout-engine`
**Spec:** XFA 3.3 §8 (p274-332), Appendix A (p1510), Appendix B (p1511-1520)
**Date:** 2026-04-07
**Reviewer:** Claude (automated spec review)

---

## Per-Section Status

| Section | Title | Status | Notes |
|---------|-------|--------|-------|
| §8.1 | Text Placement in Growable Containers | ⚠️ gap | Basic text wrap works. Missing: anchorType-based growth direction, rotated container text split |
| §8.2 | Flowing Layout (TB, LR-TB, RL-TB) | ✅ correct | All three tiled layouts + positioned implemented |
| §8.3 | hAlign in Various Layouts | ⚠️ gap | `layout_tb()` always places children at x=0. Spec says hAlign should offset child x within parent width |
| §8.4 | Growable + Flowed Interaction | ✅ correct | Resize-then-reflow pattern implemented in `layout_content_fitting()` |
| §8.5 | Layout DOM Structure | ✅ correct | Pages > contentAreas > nodes hierarchy matches spec |
| §8.6 | Layout Algorithm | ✅ correct | Content-driven single traversal with page breaks |
| §8.7 | Content Splitting | ⚠️ gap | Container-level splitting works. Missing: text-level splitting between lines, orphan/widow controls, split consensus refinement |
| §8.8 | Pagination Strategies | ⚠️ gap | Only `orderedOccurrence` implemented. Missing: `simplexPaginated`, `duplexPaginated`, qualified page selection (pagePosition, oddOrEven, blankOrNotBlank) |
| §8.9 | Adhesion (keep) | ✅ correct | Keep-chain look-ahead for `keep.contentArea` works. Missing: `keep.pageArea` level (same-page guarantee) |
| §8.10 | Leaders/Trailers | ⚠️ gap | Basic overflow leaders/trailers work. Missing: break leaders/trailers, bookend leaders/trailers, inheritance rules |
| §8.11 | Tables | ✅ correct | `columnWidths`, `colSpan`, row height equalization all implemented. Missing: rl-row (RTL), non-row direct children |
| App A | Coordinate Algorithms (anchorType) | ⚠️ gap | Always assumes TopLeft anchor. Missing: all 9 anchor types and their growth-direction algorithms |
| App B | Layout Object Characteristics | ⚠️ gap | Subform, field, draw, contentArea, pageArea handled. Missing: `area`, `exclGroup`, `subformSet` as layout-participating objects |

**Summary:** 5 ✅ correct, 8 ⚠️ gap, 0 ❌ incorrect

---

## Annotations Added

All annotations follow the format `XFA Spec 3.3 §X.Y — Title (pN): description`.

### layout.rs

| Line(s) | Annotation | Spec Reference |
|---------|-----------|----------------|
| 8-28 | Module doc: spec coverage status table | §8.1–§8.11, App A, App B |
| 152-161 | `layout()` method: content-driven traversal + pagination TODOs | §8.6 p288, §8.8 |
| 413-419 | `keep_links_content()`: adhesion algorithm + pageArea TODO | §8.9 p311-314 |
| 666-668 | Leader/trailer section in `layout_content_fitting()` | §8.10 p314-326 |
| 879-887 | `can_split()`: splitting eligibility + text-split TODO | §8.7 p290, App B p1520 |
| 937-944 | `split_tb_node()`: split consensus + keep constraints | §8.7 p290-294, §8.9 |
| 1344 | `layout_positioned()` | §8.2 positioned layout |
| 1363-1367 | `layout_tb()` + hAlign TODO | §8.2 p280, §8.3 p282 |
| 1391 | `layout_lr_tb()` | §8.2 p281 |
| 1421 | `layout_rl_tb()` | §8.2 p282 |
| 1469-1474 | `layout_table_rows()` + rl-row TODO | §8.11 p327-332 |

### types.rs

| Line(s) | Annotation | Spec Reference |
|---------|-----------|----------------|
| 217-232 | `BoxModel` doc: §8 growability table (h/w presence → growth axis) | §8 p275-276 |

### text.rs

| Line(s) | Annotation | Spec Reference |
|---------|-----------|----------------|
| 1-12 | Module doc: text placement in growable containers | §8.1 p277-279, §8.7 p291 |

---

## Fixes Applied

None. No small bugs were found that could be safely fixed within the scope of this review (annotations and comment-only changes).

The one real code gap found (§8.3 hAlign in `layout_tb()`) requires adding `hAlign` to `FormNodeMeta` and propagating it through the template parser, which is beyond a "small fix."

---

## TODOs for Larger Gaps

### High Priority (affects visual correctness)

1. **TODO §8.3** — `layout_tb()` child hAlign (`layout.rs:1367`)
   - Children in TB layout always placed at x=0. Spec says hAlign (left/center/right/justify) should offset child x within parent content width.
   - **Requires:** Add `hAlign` field to `FormNodeMeta`, parse from `<para hAlign>` in template parser, apply offset in `layout_tb()`.

2. **TODO §8.7** — Text-level content splitting (`layout.rs:887`, `text.rs:9`)
   - Text content currently treated as atomic (split at container boundaries only). Spec allows splitting text between lines when a text container spans pages.
   - **Requires:** `wrap_text()` returns line list → `split_tb_node()` splits at line boundary within text nodes.

3. **TODO §8.8** — Pagination strategies (`layout.rs:161`)
   - Only `orderedOccurrence` implemented. Missing: `simplexPaginated` (auto-insert blank pages for single-sided), `duplexPaginated` (auto-insert for duplex), qualified page selection via `pagePosition` (first/last/rest/only/any), `oddOrEven`, `blankOrNotBlank`.
   - **Requires:** Extend `layout()` page selection logic with page qualification matching.

### Medium Priority (edge cases)

4. **TODO §8.9** — `keep.pageArea` level (`layout.rs:419`)
   - Current adhesion only checks `keep.contentArea` (siblings stay in same content area). Spec also defines `keep.pageArea` (siblings stay on same page).
   - **Requires:** Track current page ID in layout state, check keep constraints against page boundary.

5. **TODO §8.10** — Break and bookend leaders/trailers (`layout.rs:668`)
   - Only overflow leaders/trailers are implemented. Spec defines three types: break (on page break), bookend (first/last page of subform), overflow (when content overflows).
   - **Requires:** Extend `FormNodeMeta` break model, add bookend tracking per subform.

6. **TODO §8.11** — RL-row and non-row table children (`layout.rs:1474`)
   - Table row placement is always LR. Spec defines `rl-row` for right-to-left cell ordering.
   - Non-row direct children of a table should be placed as if in a single-cell row.
   - **Requires:** Check layout attribute on row nodes, reverse cell x placement for rl-row.

7. **TODO App A** — anchorType coordinate algorithms
   - All layout assumes TopLeft anchor. Spec defines 9 anchor types (TopLeft, TopCenter, TopRight, MiddleLeft, MiddleCenter, MiddleRight, BottomLeft, BottomCenter, BottomRight) that affect how x,y coordinates and growth direction are interpreted.
   - **Requires:** Add `anchor_type` to `BoxModel` or `FormNodeMeta`, apply offset in all `layout_*()` methods.

### Low Priority (completeness)

8. **TODO App B** — Missing layout object types
   - `area`: transparent container, not splittable, positioned only.
   - `exclGroup`: exclusive choice group, not splittable, leaf-like.
   - `subformSet`: controls subform ordering/selection, not directly rendered.
   - **Requires:** Add to `FormNodeType`, handle in layout traversal.

9. **TODO §8.7** — Orphan/widow controls
   - Spec mentions orphan/widow considerations for text splitting. Not implemented.
   - **Requires:** Minimum-lines-before-break / minimum-lines-after-break checks in text split logic.

10. **TODO §8.1** — Rotated container text split
    - When a container has rotation, text-level splitting behavior changes (spec p279).
    - Low priority: rare in real-world forms.

---

## Compilation Status

- **xfa-layout-engine source:** ✅ No errors, passes `rustfmt --check`
- **Full workspace build:** ❌ Blocked by pre-existing `formcalc-interpreter` compile errors (15 missing builtin functions: `builtin_decode`, `builtin_encode`, `builtin_format`, etc.)
- **These errors are NOT caused by this review** — they exist on the current branch independent of our annotation changes.

---

## Architecture Notes

### Layout Engine Method Inventory (layout.rs, ~4944 LOC)

Core traversal: `layout()`, `layout_content_fitting()`, `expand_occur()`
Splitting: `can_split()`, `split_tb_node()`, `split_positioned_node()`, `has_inner_break()`
Strategy dispatch: `layout_positioned()`, `layout_tb()`, `layout_lr_tb()`, `layout_rl_tb()`, `layout_table_rows()`, `layout_row()`
Keep/adhesion: `keep_links_content()`, `visible_keep_chain_height()`, `keep_chain_height()`
Sizing: `compute_extent_with_available_and_override()`, `resolve_column_widths_with_override()`
Pagination: `estimate_page_limit()`

### Test Coverage

60+ unit tests in `layout.rs` `#[cfg(test)]` module covering:
- TB, LR-TB, RL-TB, positioned layouts
- Table with columnWidths, colSpan
- Content splitting across pages
- Keep-chain enforcement
- Growable containers (no h, no w, neither)
- Min/max constraints
- Leaders and trailers
- Pagination with page breaks

Tests cannot currently be run due to formcalc-interpreter dependency.
