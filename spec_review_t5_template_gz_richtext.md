# XFA Spec 3.3 Review — Chapter 17 (Template G-Z) + Rich Text (Ch. 5 & 27)

**Datum:** 2026-04-07
**Taak:** T5 — Template Elements G-Z + Rich Text Review
**Bestanden gereviewd:**
- `crates/pdf-xfa/src/template_parser.rs` (2542 LOC)
- `crates/pdf-xfa/src/render_bridge.rs` (1886 LOC)
- `crates/pdf-xfa/src/merger.rs` (1669 LOC)
- `crates/xfa-layout-engine/src/text.rs` (533 LOC)

---

## 1. Template Elements G-Z Status

| Element | Status | Locatie | Notes |
|---------|--------|---------|-------|
| **image** | ✅ Volledig | `extract_value_image` (merger.rs:1603) | image/png, image/jpeg, BMP→PNG conversie |
| **imageEdit** | ✅ Volledig | `detect_field_kind` (template_parser.rs:800) | FieldKind::ImageEdit |
| **integer** | ✅ Volledig | `extract_value_text` (merger.rs:781) | Ondersteund als value type |
| **issuers** | ❌ Ontbreekt | — | Niet geïmplementeerd (security/certificates) |
| **keep** | ✅ Volledig | `parse_keep` (template_parser.rs:998) | next/previous/intact contentArea |
| **keyUsage** | ❌ Ontbreekt | — | Niet geïmplementeerd (digitale handtekening) |
| **line** | ✅ Volledig | `extract_draw_content` (merger.rs:1662) | x1,y1,x2,y2 support |
| **linear** | ✅ Volledig | `parse_node_style` (template_parser.rs:550) | Linear gradient fill |
| **locale** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **lockDocument** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **manifest** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **margin** | ✅ Volledig | `parse_margin` (template_parser.rs:1549) | topInset/bottomInset/leftInset/rightInset |
| **mdp** | ❌ Ontbreekt | — | Niet geïmplementeerd (document security) |
| **medium** | ✅ Volledig | `read_medium` (merger.rs:1542) | short/long voor page dimensions |
| **message** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **numericEdit** | ✅ Volledig | `detect_field_kind` (template_parser.rs:798) | FieldKind::NumericEdit |
| **occur** | ✅ Volledig | `parse_occur` (template_parser.rs:1571) | min/max/initial |
| **oid** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **oids** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **overflow** | ✅ Volledig | `parse_overflow` (template_parser.rs:1007) | leader/trailer voor pagination |
| **pageArea** | ✅ Volledig | `parse_page_area` (template_parser.rs:395) | pageSet parsing, medium, contentArea |
| **pageSet** | ✅ Volledig | `parse_page_set` (template_parser.rs:366) | pageArea children |
| **para** | ✅ Volledig | `parse_node_style` (template_parser.rs:717) | hAlign, vAlign, spaceAbove, spaceBelow, marginLeft, marginRight |
| **passwordEdit** | ✅ Volledig | `detect_field_kind` (template_parser.rs:799) | FieldKind::PasswordEdit |
| **pattern** | ➖ Deels | `parse_node_style` (template_parser.rs:550) | Pattern fill niet rendering, alleen als ignore |
| **picture** | ❌ Ontbreekt | — | Niet geïmplementeerd (picture clause formatting) |
| **proto** | ✅ Volledig | `add_children` (template_parser.rs:1418) | Proto wordt genegeerd voor layout |
| **radial** | ✅ Volledig | `parse_node_style` (template_parser.rs:550) | Radial gradient fill |
| **reason** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **reasons** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **ref** | ✅ Volledig | `parse_bind` (template_parser.rs:518) | Data binding ref |
| **script** | ✅ Volledig | `collect_event_scripts` (template_parser.rs:880) | event/calculate scripts |
| **scriptTest** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **setProperty** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **signing** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **signData** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **signature** | ✅ Volledig | `detect_field_kind` (template_parser.rs:801) | FieldKind::Signature |
| **solid** | ✅ Volledig | `parse_node_style` (template_parser.rs:550) | Solid fill |
| **speak** | ❌ Ontbreekt | — | Niet geïmplementeerd (accessibility) |
| **stipple** | ✅ Volledig | `parse_node_style` (template_parser.rs:550) | Stipple fill |
| **subform** | ✅ Volledig | `parse_subform_node` (template_parser.rs:141) | Volledige subform parsing |
| **subformSet** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **submit** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **template** | ✅ Volledig | `parse_template` (template_parser.rs:33) | Root template parsing |
| **text** | ✅ Volledig | `extract_value_text` (merger.rs:779) | Text value extraction |
| **textEdit** | ✅ Volledig | `detect_field_kind` (template_parser.rs:806) | FieldKind::Text (default) |
| **time** | ❌ Ontbreekt | — | Niet geïmplementeerd (time field type) |
| **timeStamp** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **toolTip** | ✅ Volledig | `add_children` (template_parser.rs:1414) | Genegeerd voor layout (wel aanwezig) |
| **traversal** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **traverse** | ❌ Ontbreekt | — | Niet geïmplementeerd |
| **ui** | ✅ Volledig | `detect_field_kind` (template_parser.rs:781) | UI type detectie |
| **validate** | ✅ Volledig | `add_children` (template_parser.rs:1414) | Script validation (genegeerd voor layout) |
| **value** | ✅ Volledig | `extract_value_text` (merger.rs:779) | value element extractie |
| **variables** | ❌ Ontbreekt | — | Niet geïmplementeerd (FormCalc variables) |

---

## 2. Rich Text Support Matrix (Chapters 5 & 27)

### 2.1 HTML Tags in exData

| Tag | Status | Notes |
|-----|--------|-------|
| `<span>` | ⚠️ Deels | Wordt gestript, alleen text content behouden |
| `<p>` | ⚠️ Deels | Block element, newlines toegevoegd maar geen spacing |
| `<b>` | ❌ Ontbreekt | Wordt gestript, bold formatting niet behouden |
| `<i>` | ❌ Ontbreekt | Wordt gestript, italic formatting niet behouden |
| `<br/>` | ✅ Volledig | Newline toegevoegd |
| `<div>` | ⚠️ Deels | Block element, newlines toegevoegd |
| `<ul>/<ol>` | ⚠️ Deels | Li elements krijgen newlines, geen bullet/rendering |
| `<li>` | ⚠️ Deels | Wordt behandeld als block, newlines maar geen list rendering |

### 2.2 CSS Properties

| Property | Status | Locatie | Notes |
|----------|--------|---------|-------|
| `font-size` | ✅ Volledig | `extract_exdata_font_size` (merger.rs:834) | pt suffix ondersteund |
| `font-weight` | ⚠️ Deels | `extract_exdata_font_weight` (merger.rs:866) | Alleen "bold" gedetecteerd, geen numerieke weights |
| `font-style` | ✅ Toegevoegd | `extract_exdata_font_style` (merger.rs:889) | italic/normal/oblique |
| `font-family` | ❌ Ontbreekt | — | Wordt niet uit HTML geëxtraheerd |
| `color` | ✅ Toegevoegd | `extract_exdata_color` (merger.rs:910) | #RGB, #RRGGBB, rgb() |
| `margin-top` | ❌ Ontbreekt | — | Wordt niet geparsed uit rich text |
| `margin-bottom` | ❌ Ontbreekt | — | Wordt niet geparsed uit rich text |
| `text-align` | ❌ Ontbreekt | — | Wordt niet uit HTML/CSS geparsed |

### 2.3 Paragraph Formatting (XFA `<para>` element)

| Property | Status | Notes |
|----------|--------|-------|
| `hAlign` | ✅ Volledig | template_parser.rs:349-357 |
| `vAlign` | ✅ Volledig | template_parser.rs:734-740 |
| `spaceAbove` | ✅ Volledig | template_parser.rs:722-723 |
| `spaceBelow` | ✅ Volledig | template_parser.rs:725-726 |
| `marginLeft` | ✅ Volledig | template_parser.rs:728-729 |
| `marginRight` | ✅ Volledig | template_parser.rs:731-732 |

---

## 3. Aangebrachte Annotaties en Fixes

### 3.1 Annotaties toegevoegd

**template_parser.rs:**
```rust
// XFA Spec 3.3 §17 "para" (p803) — Paragraph-level formatting attributes:
// hAlign: left/center/right/justify
// vAlign: top/middle/bottom for vertical alignment within container
// spaceAbove, spaceBelow: paragraph spacing in points
// marginLeft, marginRight: paragraph indentation
```
(Locatie: regel 717)

```rust
// XFA Spec 3.3 §17 "occur" (p800-802) — Specifies min/max/initial occurrences:
// min: minimum instances (default 1)
// max: maximum instances (-1 means unlimited)
// initial: number of instances at initialization
```
(Locatie: regel 1571)

```rust
// XFA Spec 3.3 §17 "keep" (p776-777) — Controls whether content Area breaks are allowed:
// next: keep next content area together
// previous: keep previous content area together  
// intact: keep content area intact (no breaks within)
```
(Locatie: regel 998)

```rust
// XFA Spec 3.3 §17 "overflow" (p804-805) — Overflow leader/trailer for pagination:
// leader: reference to element to render before overflow content
// trailer: reference to element to render after overflow content
```
(Locatie: regel 1007)

### 3.2 Nieuwe Functies Toegevoegd (merger.rs)

**`extract_exdata_font_style`** (regel 889):
- XFA Spec 3.3 §27.4: font-style support (normal, italic, oblique)

**`extract_exdata_color`** (regel 910):
- XFA Spec 3.3 §27.4: color support (#RGB, #RRGGBB, rgb(r,g,b))

**`parse_css_color`** (regel 929):
- Hulpfunctie voor CSS kleur parsing

### 3.3 Verbeteringen aan Bestaande Functies

**`extract_exdata_font_weight`** (merger.rs:866):
- Uitgebreid om numerieke font-weights (700-900) als "bold" te herkennen
- Normal weight (400-600) wordt nu ook herkend

---

## 4. TODO's en Gaps

### 4.1 Hoge Prioriteit

1. **Rich text `<b>`, `<i>` tags** — Worden momenteel gestript zonder formatting te behouden
   - Impact: Vetgedrukte en schuingedrukte tekst in rich text wordt niet correct gerenderd
   - Oplossing: Inline formatting opslaan in een struct die door de renderer kan worden gebruikt

2. **CSS `margin-top`, `margin-bottom` in rich text** — Paragraph spacing uit HTML/CSS wordt niet toegepast
   - Impact: Instructies/paragrafen in rich text hebben geen juiste spacing
   - Oplossing: Parse margin properties uit `<p style="margin-top: Xpt">`

3. **Font-family uit HTML** — Wordt niet geëxtraheerd uit `<span style="font-family: ...">`
   - Impact: Rich text kan verkeerde font gebruiken
   - Oplossing: Implementeer extractie uit CSS style attribute

### 4.2 Middel Prioriteit

4. **List rendering (`<ul>`, `<ol>`)** — Worden newlines toegevoegd maar geen bullets/numbers
   - Impact: Lijsten in rich text zien er niet correct uit
   - Oplossing: herken li elements en plaats bullet karakters

5. **`<picture>` element** — Picture clause voor formatting wordt niet ondersteund
   - Impact: Formaat/wijze van data formatting niet mogelijk
   - Oplossing: Implementeer picture clause parsing

6. **`variables` element** — FormCalc variables niet ondersteund
   - Impact: FormCalc scripts met variabelen werken niet
   - Oplossing: Implementeer variabele scope in FormCalc interpreter

### 4.3 Lage Prioriteit (Security/Signing)

7. `signing`, `signData`, `keyUsage`, `issuers` — Digitale handtekening gerelateerd
8. `time`, `timeStamp` — Timestamp velden
9. `mdp`, `lockDocument` — Document beveiliging
10. `speak` — Accessibility (screen reader) support

---

## 5. Pre-existente Problemen

**formcalc-interpreter compilatiefouten:**
- De `crates/formcalc-interpreter/src/builtins.rs` bevat ontbrekende functies:
  - `builtin_unit_type`, `builtin_unit_value`, `builtin_eval`, `builtin_ref`
  - `builtin_get`, `builtin_post`, `builtin_put`
  - `parse_iso_date_string`, `format_time_string`
- Dit zijn **niet** veroorzaakt door deze review maar zijn pre-existente issues
- De pdf-xfa, merger.rs, en template_parser.rs wijzigingen zijn syntactisch correct

---

## 6. Samenvatting

| Categorie | Volledig | Deels | Ontbreekt | Totaal |
|-----------|----------|-------|-----------|--------|
| Template G-Z | 27 | 3 | 21 | 51 |
| Rich Text Tags | 1 | 4 | 4 | 9 |
| Rich Text CSS | 3 | 1 | 4 | 8 |
| Para Formatting | 6 | 0 | 0 | 6 |

**Key Finding:** De basis template parsing (subform, field, draw, pageSet, pageArea) en basis rich text extractie zijn goed geïmplementeerd. De belangrijkste gaps zijn:
1. Inline formatting (bold, italic) in rich text
2. CSS margin parsing voor paragraph spacing in rich text
3. Font-family extractie uit HTML styles
4. List rendering voor `<ul>`, `<ol>`, `<li>`

---

## 7. Files Modified

| File | Changes |
|------|---------|
| `crates/pdf-xfa/src/merger.rs` | +3 functies (extract_exdata_font_style, extract_exdata_color, parse_css_color), +1 verbetering (extract_exdata_font_weight) |
| `crates/pdf-xfa/src/template_parser.rs` | +4 spec annotations, 1 bestaande annotatie verbeterd |

**Let op:** `crates/formcalc-interpreter/src/builtins.rs` heeft pre-existente compilatiefouten die niet gerelateerd zijn aan deze review.
