# Spec Review T2: XFA 3.3 Chapter 4 — Data Binding

Review of XFA Spec 3.3 Chapter 4 (p122-214) against `merger.rs` and `data_dom.rs`.

## Per-Section Assessment

### 4.1 — Loading Data into the Data DOM (p122-142)

| Sub-topic | Status | Notes |
|---|---|---|
| DataGroup vs DataValue classification | ✅ | `build_from_xml_node` correctly uses `has_element_children` (p126) |
| Attributes as DataValue children | ✅ **FIX** | Was `DataContains::Data`, fixed to `DataContains::MetaData` (p127) |
| `xsi:nil="true"` handling | ✅ | Correctly detected and stored as `NullType::Xsi` (p139) |
| Namespace exclusion (`excludeNS`) | ❌ | Not implemented (p134). TODO added. |
| `contentType` attribute on data values | ✅ | Parsed from XML attribute |
| Datasets packet unwrapping | ✅ | `unwrap_datasets_root` handles single and double-wrapped `<xfa:data>` |
| Mixed content (text + elements) | ⚠️ | Element children win; text content of mixed elements is silently dropped |

### 4.2 — Localization and Canonicalization (p143-170)

| Sub-topic | Status | Notes |
|---|---|---|
| Picture clauses for formatting | ❌ | Not implemented. TODO in `add_children`. |
| Locale-specific data parsing | ❌ | Not implemented. Values used as-is. |
| Data canonicalization on load | ❌ | Not implemented. |
| Data canonicalization on save | ❌ | Not implemented. TODO in `to_xml`. |

### 4.3 — Saving Data from the Data DOM (p171-175)

| Sub-topic | Status | Notes |
|---|---|---|
| XML serialization | ✅ | `to_xml()` produces valid XML |
| `xsi:nil` output | ✅ | Correctly emits `xsi:nil="true"` with namespace declaration |
| Canonicalization on save | ❌ | TODO added in `to_xml` doc comment |
| `saveFormat` attribute | ❌ | Not implemented |

### 4.4 — Merging Data with a Template (p176-214)

#### 4.4.1 — Merge Modes (p176)

| Sub-topic | Status | Notes |
|---|---|---|
| `consumeData` mode | ✅ | Implemented — template drives the merge |
| `matchTemplate` mode | ❌ | Not implemented. TODO in module doc + `merge()`. |

#### 4.4.2 — `<bind>` Element (p176)

| Sub-topic | Status | Notes |
|---|---|---|
| `match="once"` (default) | ✅ | Direct match implemented |
| `match="none"` | ✅ | `parse_bind` detects and `data_bind_none` flag used |
| `match="dataRef"` | ⚠️ | Simple `$.name[*]` patterns work; full SOM evaluation missing |
| `match="global"` | ❌ | Not implemented. TODO in `parse_bind`. Treated as `once`. |

#### 4.4.3 — Data Binding Algorithm (p180-185)

| Sub-topic | Status | Notes |
|---|---|---|
| Step 1: Create form nodes + direct match | ✅ | `parse_node` creates form nodes, searches context children |
| Direct match (children by name) | ✅ | `children_by_name` in both subform and field binding |
| Scope matching (ancestor match) | ❌ | Not implemented. TODO in subform binding block. |
| Scope matching (sibling match) | ❌ | Not implemented. |
| Root subform → data root binding | ✅ | §4.7.2 — name match check implemented |
| Fallback to first child group | ⚠️ | Non-spec heuristic, but works for common real-world patterns |
| Global descendant search (fallback) | ⚠️ | `find_value_in_descendants` approximates but differs from spec |

#### 4.4.4 — Repeating Subforms (p186-192)

| Sub-topic | Status | Notes |
|---|---|---|
| Instance count from data records | ✅ | `expand_repeating_subform` counts matching data children |
| Clamping to [min, max] | ✅ | Correctly applied |
| `<occur>` parsing (min/max/initial) | ✅ | §9.2 defaults correctly handled |
| Bind ref for data name | ✅ | `parse_bind_data_name` extracts name from `$.name[*]` |
| Instance container creation | ✅ | `{name}_container` wrapper node |

#### 4.4.5 — Transparent Nodes (p193)

| Sub-topic | Status | Notes |
|---|---|---|
| Nameless subforms transparent to binding | ⚠️ | Data context passes through, but full transparency semantics missing. TODO added. |

#### 4.4.6 — Exclusion Groups (p195)

| Sub-topic | Status | Notes |
|---|---|---|
| Short format (single data value) | ⚠️ | Implemented in `template_parser.rs` `bind_data`, not in `merger.rs` |
| Long format (per-field data) | ❌ | Not implemented |

#### 4.4.7 — Attribute Matching (p197)

| Sub-topic | Status | Notes |
|---|---|---|
| Unbound data attributes → fields | ❌ | Not implemented. TODO in `parse_field`. |

#### 4.4.8 — Re-normalization (p198)

| Sub-topic | Status | Notes |
|---|---|---|
| Excess data creates new form nodes | ❌ | Not implemented. TODO in `add_children`. |

#### 4.4.9 — Explicit Data References (p199-201)

| Sub-topic | Status | Notes |
|---|---|---|
| `<bind match="dataRef" ref="...">` | ⚠️ | Simple patterns work; full SOM missing. TODO in `parse_bind_data_name`. |
| `setProperty` | ❌ | Not implemented. TODO in `add_children`. |
| `bindItems` | ❌ | Not implemented. TODO in `add_children`. |

#### 4.4.10 — Post-Merge Steps (p202-214)

| Sub-topic | Status | Notes |
|---|---|---|
| Bind to properties (step 5) | ❌ | Not implemented |
| Calculations and validations (step 6) | ⚠️ | FormCalc scripts collected but executed separately in `dynamic.rs` |
| Form ready event (step 7) | ⚠️ | Event scripts collected; execution handled elsewhere |
| Remerge (step 8) | ❌ | Not implemented |

## Summary

| Category | ✅ Implemented | ⚠️ Partial | ❌ Missing | Total |
|---|---|---|---|---|
| §4.1 Data Loading | 5 | 1 | 1 | 7 |
| §4.2 Localization | 0 | 0 | 4 | 4 |
| §4.3 Data Saving | 2 | 0 | 2 | 4 |
| §4.4 Merging | 10 | 6 | 10 | 26 |
| **Total** | **17** | **7** | **17** | **41** |

**Coverage: 41% fully implemented, 59% partial or missing.**

The core data binding path (direct match, repeating subforms, bind ref) works correctly
and handles real-world XFA forms. The main gaps are in advanced features (scope matching,
localization, re-normalization, matchTemplate mode) that are rarely exercised by typical
forms.

## Changes Made

### Bug Fix
- **data_dom.rs:158** — `DataContains::Data` → `DataContains::MetaData` for XML attributes.
  Per XFA Spec 3.3 §4.1 p127: "An attribute on an element in the data is described by a
  dataValue node where the contains property has the value 'metaData'."

### Annotations Added (spec references)

**data_dom.rs:**
- `NullType` enum — §4.1 p139
- `DataContains` enum — §4.1 p127
- `from_xml()` — §4.1 p122-142 (loading rules)
- `build_from_xml_node()` — §4.1 p126 (classification rule)
- Attribute handling — §4.1 p127/142 (metaData)
- Leaf elements — §4.1 p126 ("contains only character data")
- `xsi:nil` detection — §4.1 p139
- `unwrap_datasets_root()` — §4.1 p122 (datasets packet structure)
- `to_xml()` — §4.3 p171-175 (saving data)

**merger.rs:**
- Module doc — §4.4 p176-214 (overview + gap list)
- `merge()` — §4.4 p176 (consumeData mode)
- Subform binding block — §4.4.3 p180-185 (direct match)
- Root subform binding — §4.7.2
- `expand_repeating_subform()` — §4.4 p186-192
- `find_value_in_descendants()` — §4.4.3 p185 (global search)
- `parse_field()` — §4.4.3 p180 (field binding)
- `parse_draw()` — §4.4 p180 (draw = no binding)
- `add_children()` — §4.4 p180 (merge walk)
- `parse_occur()` — §4.4 p186 + §9.2 p357
- `parse_bind_data_name()` — §4.4 p199-201 (explicit data refs)
- `parse_bind()` — §4.4.3 p176 (bind match types)

### TODOs Added

| Location | Spec Reference | Gap |
|---|---|---|
| data_dom.rs `from_xml()` | §4.1 p134 | Namespace exclusion rules |
| data_dom.rs `to_xml()` | §4.3 p171 | Canonicalization via picture clauses |
| merger.rs module doc | §4.4 p176 | `matchTemplate` merge mode |
| merger.rs subform binding | §4.4.3 p185 | Scope matching (ancestor/sibling) |
| merger.rs subform binding | §4.4 p193 | Full transparent node semantics |
| merger.rs `find_value_in_descendants` | §4.4.3 p176 | `bind match="global"` |
| merger.rs `parse_field` | §4.4.3 p185 | Scope matching before global fallback |
| merger.rs `parse_field` | §4.4 p197 | Attribute matching step |
| merger.rs `add_children` | §4.4 p198 | Re-normalization |
| merger.rs `add_children` | §4.4 p199 | setProperty/bindItems |
| merger.rs `add_children` | §4.2 p143 | Localization/canonicalization |
| merger.rs `parse_bind_data_name` | §4.4 p199 | Full SOM expression evaluation |
| merger.rs `parse_bind` | §4.4.3 p176 | `match="global"` handling |

## Verification

- `cargo test -p pdf-xfa`: **108 passed**, 0 failed
- `cargo test -p xfa-dom-resolver`: **32 + 15 passed**, 0 failed
- `cargo check -p pdf-xfa -p xfa-dom-resolver`: clean (0 warnings)
- Pre-existing clippy errors in `lopdf` and `formcalc-interpreter` only
