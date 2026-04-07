# XFA Spec 3.3 Chapter 17 Review — Template Parser (Elementen A–F)

**Datum:** 2026-04-07
**Parser:** `crates/pdf-xfa/src/template_parser.rs` (2479 LOC)
**Review scope:** p596–740 (Guide + elementen A–F)

---

## Samenvatting

| Element | Status | Opmerking |
|---------|--------|-----------|
| `arc` | ⚠️ Deels | `x y w h startAngle sweepAngle` geparsed, maar `initAngle`, `fill`, `edge` attributes ontbreken |
| `area` | ❌ Ontbreekt | Wordt genegeerd (geen eigen FormNode) |
| `assist` | ⚠️ Deels | `toolTip` via `<toolTip>` child, maar `role`, `name`, `use`, `usehref` ontbreken |
| `barcode` | ⚠️ Deels | FieldKind::Barcode gedetecteerd, maar `barcodeType`, `checksum`, `dataLength`, `moduleWidth`, etc. niet geparsed |
| `bind` | ✅ Volledig | `ref` en `match="none"` correct |
| `bindItems` | ❌ Ontbreekt | `ref` en `picture` niet geïmplementeerd |
| `bookend` | ❌ Ontbreekt | Wordt volledig genegeerd |
| `boolean` | ❌ Ontbreekt | `value` content niet als boolean geïnterpreteerd |
| `border` | ⚠️ Deels | `edge`, `corner`, `fill` children geparsed, maar `borderType`, `joinStyle`, `presence` op border element zelf ontbreken |
| `break` | ✅ Volledig | `before`, `after`, `target`, `targetType` correct |
| `breakAfter` | ✅ Volledig | `target`, `targetType` correct |
| `breakBefore` | ✅ Volledig | `target`, `targetType` correct |
| `button` | ❌ Ontbreekt | Geen buttonType, caption handling voor buttons |
| `calculate` | ⚠️ Deels | `<script>` child correct, maar `format`, `messge`, `use`, `usehref` attributen ontbreken |
| `caption` | ⚠️ Deels | `placement`, `reserve`, `value` correct, maar `use`, `usehref`, `startPos` ontbreken |
| `certificate` | ❌ Ontbreekt | Wordt genegeerd |
| `certificates` | ❌ Ontbreekt | Wordt genegeerd |
| `checkButton` | ⚠️ Deels | `shape="round"` → Radio, anders Checkbox; maar `force`, `mark`, `shape` (anders dan round) niet in style |
| `choiceList` | ⚠️ Deels | FieldKind::Dropdown correct, maar `open`, `textEntry`, `commitOn` ontbreken |
| `color` | ⚠️ Deels | `value="r,g,b"` geparsed, maar `cSpace`, `exData` niet |
| `comb` | ❌ Ontbreekt | `maxChars` niet geparsed voor field width distributie |
| `command` | ❌ Ontbreekt | `connectionName`, `dataSource`, `queryString` niet geïmplementeerd |
| `connect` | ⚠️ Deels | Alleen `connectionName`; `ref`, `use`, `usehref`, `relation` ontbreken |
| `contentArea` | ✅ Volledig | `name`, `x`, `y`, `w`, `h` correct |
| `corner` | ⚠️ Deels | `radius` geparsed, maar `joinStyle`, `stroke`, `presence` ontbreken |
| `date` | ⚠️ Deels | Content via `extract_value_text`, maar `datePattern`, `use`, `usehref` ontbreken |
| `dateTime` | ⚠️ Deels | Content via `extract_value_text`, maar `datePattern`, `use`, `usehref` ontbreken |
| `dateTimeEdit` | ⚠️ Deels | FieldKind::DateTimePicker correct, maar `pickerFormat`, `use`, `usehref` ontbreken |
| `decimal` | ⚠️ Deels | Content via `extract_value_text`, maar `fracDigits`, `leadDigits`, `symbol` ontbreken |
| `defaultUi` | ❌ Ontbreekt | Wordt genegeerd |
| `desc` | ✅ Volledig | Description text niet opgeslagen maar mag genegeerd worden |
| `digestMethod` | ❌ Ontbreekt | `method` attribut niet geparsed |
| `digestMethods` | ❌ Ontbreekt | Wordt genegeerd |
| `draw` | ⚠️ Deels | `name`, box model, `value` (text/image/line/rect/arc) correct; `rotate`, `rawContent`, `accessibleCaption` ontbreken |
| `edge` | ⚠️ Deels | `stroke`, `thickness`, `color` correct; `cap`, `joinStyle`, `presence` ontbreken |
| `encoding` | ❌ Ontbreekt | `encodingName`, `desc` niet geïmplementeerd |
| `encodings` | ❌ Ontbreekt | Wordt genegeerd |
| `encrypt` | ❌ Ontbreekt | Wordt genegeerd |
| `event` | ⚠️ Deels | `activity`, `ref`, `runAt` via `<script>`; maar `name`, `listener`, `xfa:contentType`, `level` ontbreken |
| `exData` | ⚠️ Deels | `contentType` gebruikt voor text/html extraction; `maxLength`, `rid`, `href` ontbreken |
| `exObject` | ❌ Ontbreekt | `className`, `contentType`, `href` niet geïmplementeerd |
| `exclGroup` | ✅ Volledig | `name`, `layout`, box model, `occur`, `bind`, children correct; wordt ExclusiveChoice |
| `execute` | ❌ Ontbreekt | `runAt`, `use` attributen niet geïmplementeerd |
| `extras` | ⚠️ Deels | `when`, `label` als string opslaan; maar `use`, `usehref` ontbreken |
| `field` | ⚠️ Deels | `name`, box model, `occur`, `caption`, `ui`, `value`, `items`, `font`, `border`, `margin`, `para` correct; `access`, `accessKey`, `anchorType`, `hAlign`, `relevant`, `rotate`, `use`, `usehref`, `locale` ontbreken |
| `fill` | ⚠️ Deels | `<color>`, `<solid><color>` voor bg_color; `linear`, `radial`, `stipple`, `pattern` niet |
| `filter` | ❌ Ontbreekt | `logic`, `dataFilter` type attributen niet geïmplementeerd |
| `float` | ⚠️ Deels | Content via `extract_value_text`; `dataPrecision`, `symbol`, `use` ontbreken |
| `font` | ⚠️ Deels | `typeface`, `size`, `weight`, `posture`, `color`, `fontHorizontalScale`, `letterSpacing` correct; `fontType`, `purpose`, `encoding`, `glyphlist`, `actual`, `embed`, `status` ontbreken |
| `format` | ⚠️ Deels | `category`, `crack` in `<event>`/`validate`; `picture` pattern (belangrijk voor XFA forms) ontbreekt |
| `area` (second) | ❌ Ontbreekt | Zie boven |

---

## Detailanalyse per Element

### `arc` (p602–604)
**XFA Spec 3.3 §17 "arc"**
- **Attributen:** `name`, `id`, `x`, `y`, `w`, `h`, `startAngle`, `sweepAngle`, `initAngle`, `fill`, `edge`, `presence`, `relevant`, `use`, `usehref`
- **Wij behandelen:** `x`, `y`, `w`, `h`, `startAngle`, `sweepAngle` via `extract_draw_content` → `DrawContent::Arc`
- **Ontbrekend:** `initAngle`, `fill` (solid/linear/radial/...), `edge` override, `presence`, `relevant`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "arc" (p602) — Attributes: name, id, x, y, w, h, startAngle, sweepAngle, initAngle, fill, edge, presence, relevant, use, usehref. We parse: [x, y, w, h, startAngle, sweepAngle]. Missing: [initAngle, fill, edge, presence, relevant, use, usehref].`

### `area` (p605–606)
**XFA Spec 3.3 §17 "area"**
- **Attributen:** `name`, `id`, `x`, `y`, `w`, `h`, `的存在`, `presence`, `relevant`, `use`, `usehref`
- **Wij behandelen:** Niets — `area` valt onder `_` in `parse_node` en wordt `blank_node` (Subform)
- **Ontbrekend:** Alle attributen — `area` wordt genegeerd behalve als generieke container
- **Annotatie:** `// XFA Spec 3.3 §17 "area" (p605) — COMPLETELY MISSING. Area is a child of subform for conditional content area assignment. Should parse as FormNode with special layout behavior.`

### `assist` (p607–608)
**XFA Spec 3.3 §17 "assist"**
- **Attributen:** `role`, `name`, `use`, `usehref`, `toolTip`
- **Wij behandelen:** `<toolTip>` child wordt genegeerd in `add_children` (regel 1415)
- **Ontbrekend:** `role`, `name`, `use`, `usehref`, `toolTip` (text content)
- **Notitie:** `toolTip` is nuttig voor accessibility maar niet cruciaal voor rendering

### `barcode` (p609–616)
**XFA Spec 3.3 §17 "barcode"**
- **Attributen:** `barcodeType`, `checksum`, `dataLength`, `moduleWidth`, `moduleHeight`, `truncate`, `textLocation`, `startChar`, `endChar`, `ucData`, `x`, `y`, `w`, `h`, `presence`, `relevant`, `use`, `usehref`, `rotate`, `enableSoft默认值`
- **Wij behandelen:** Wordt gedetecteerd als `FieldKind::Barcode` in `detect_field_kind` (regel 802)
- **Ontbrekend:** Alle specifieke barcode attributen — geen barcode symbologie, checksum, of dimensionale info
- **Annotatie:** `// XFA Spec 3.3 §17 "barcode" (p609) — FieldKind::Barcode detected but no barcode attributes parsed (barcodeType, checksum, dataLength, moduleWidth/Height, truncate, textLocation, startChar, endChar, ucData, rotate, enableSoftDefault). Missing: all barcode-specific attributes.`

### `bind` (p617–618)
**XFA Spec 3.3 §17 "bind"**
- **Attributen:** `ref`, `match`
- **Wij behandelen:** ✅ Volledig — `parse_bind` (regel 518) pakt `ref` en `match="none"`
- **Notitie:** Correcte implementatie

### `bindItems` (p619)
**XFA Spec 3.3 §17 "bindItems"**
- **Attributen:** `ref`, `picture`, `connectionName`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets — niet geparsed
- **Ontbrekend:** `ref` (data binding voor list items), `picture` (value formatting)
- **Notitie:** Wordt gebruikt voor dynamic dropdowns waar items aan data gebonden worden

### `bookend` (p620)
**XFA Spec 3.3 §17 "bookend"**
- **Attributen:** `leader`, `trailer`, `short`, `flags`
- **Wij behandelen:** ❌ Niets — niet genoemd in `add_children`
- **Ontbrekend:** `leader` (reference to draw element), `trailer`, `short` (style), `flags`

### `boolean` (p621)
**XFA Spec 3.3 §17 "boolean"**
- **Content:** `true()` of `false()` functie
- **Wij behandelen:** ❌ Content wordt via `extract_value_text` als string "true()" of "false()" behandeld, niet als boolean
- **Notitie:** Lage prioriteit — velden met boolean content zijn zeldzaam

### `border` (p622–628)
**XFA Spec 3.3 §17 "border"**
- **Attributen:** `borderType`, `joinStyle`, `presence`, `relevant`, `use`, `usehref`, `w`, `h`
- **Children:** `edge`+, `corner`+, `fill`
- **Wij behandelen:** `edge` (kleur, stroke, thickness), `corner` (radius), `fill` (bg_color via border fill)
- **Ontbrekend:** `borderType` (type style), `joinStyle` (corner join), `presence` op border, `w`, `h` (dimensions)
- **Annotatie:** `// XFA Spec 3.3 §17 "border" (p622) — Attributes: borderType, joinStyle, presence, relevant, use, usehref, w, h. Children: edge+, corner+, fill. We parse: edge (stroke, thickness, color), corner (radius), fill (bg_color). Missing: borderType, joinStyle, presence on border, w, h.`

### `break` (p629–630)
**XFA Spec 3.3 §17 "break"**
- **Attributen:** `before`, `after`, `target`, `targetType`, `use`, `usehref`
- **Wij behandelen:** ✅ Volledig — `detect_page_break_before` en `detect_page_break_after` via legacy `break` handling (regels 828, 856)

### `breakAfter` (p631)
**XFA Spec 3.3 §17 "breakAfter"**
- **Attributen:** `target`, `targetType`, `use`, `usehref`
- **Wij behandelen:** ✅ Volledig (regel 851–854)

### `breakBefore` (p632)
**XFA Spec 3.3 §17 "breakBefore"**
- **Attributen:** `target`, `targetType`, `use`, `usehref`
- **Wij behandelen:** ✅ Volledig (regel 823–826)

### `button` (p633–636)
**XFA Spec 3.3 §17 "button"**
- **Attributen:** `buttonType`, `caption`, `highlight`, `open`, `presence`, `rollover`, `sound`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets — `button` valt onder `_` in `detect_field_kind`
- **Ontbrekend:** `buttonType` (link, submit, reset, calculate), `caption` (text), `highlight` (mode), `open` (voor flyover), `rollover`, `sound`

### `calculate` (p637–638)
**XFA Spec 3.3 §17 "calculate"**
- **Attributen:** `message`, `use`, `usehref`, `format`, `crack`
- **Children:** `script`+
- **Wij behandelen:** `<script>` child wordt via `collect_event_scripts` (regel 900) geparsed
- **Ontbrekend:** `message`, `use`, `usehref`, `format` (picture pattern), `crack`
- **Annotatie:** `// XFA Spec 3.3 §17 "calculate" (p637) — Attributes: message, use, usehref, format, crack. We parse script child for event_scripts. Missing: [message, use, usehref, format, crack].`

### `caption` (p639–640)
**XFA Spec 3.3 §17 "caption"**
- **Attributen:** `placement`, `reserve`, `startPos`, `use`, `usehref`, `relevant`, `presence`
- **Children:** `value`
- **Wij behandelen:** ✅ `placement`, `reserve`, `value` via `parse_caption` (regel 1787)
- **Ontbrekend:** `startPos` (character position for caption text), `use`, `usehref`, `relevant`, `presence`
- **Annotatie:** `// XFA Spec 3.3 §17 "caption" (p639) — Attributes: placement, reserve, startPos, use, usehref, relevant, presence. We parse: [placement, reserve, value/text]. Missing: [startPos, use, usehref, relevant, presence].`

### `certificate` (p641–644)
**XFA Spec 3.3 §17 "certificate"`
- **Attributen:** `encoding`, `filter`, `loadRemote`, `name`, `password`, `sigType`, `subject`, `type`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets — niet genoemd in parser
- **Notitie:** Voor digitale signatures — lage prioriteit tenzij signature velden ondersteund worden

### `certificates` (p645)
**XFA Spec 3.3 §17 "certificates"**
- **Attributen:** `use`, `usehref`, `ref`
- **Children:** `certificate`+
- **Wij behandelen:** ❌ Niets
- **Notitie:** Idem als certificate

### `checkButton` (p646–652)
**XFA Spec 3.3 §17 "checkButton"**
- **Attributen:** `shape`, `mark`, `size`, `style`, `textLocation`, `force`, `use`, `usehref`
- **Wij behandelen:** ⚠️ `shape="round"` → `FieldKind::Radio`, anders `FieldKind::Checkbox` (regel 789)
- **Ontbrekend:** `mark` (check type: diamond, cross, etc.), `size`, `style`, `textLocation`, `force`
- **Notitie:** Style attributen voor checkButton bepalen visuals maar zijn secundair
- **Annotatie:** `// XFA Spec 3.3 §17 "checkButton" (p646) — Attributes: shape, mark, size, style, textLocation, force, use, usehref. We parse: [shape="round"→Radio, else→Checkbox]. Missing: [mark, size, style, textLocation, force, use, usehref].`

### `choiceList` (p653–656)
**XFA Spec 3.3 §17 "choiceList"**
- **Attributen:** `open`, `textEntry`, `commitOn`, `use`, `usehref`
- **Wij behandelen:** `FieldKind::Dropdown` gedetecteerd (regel 796)
- **Ontbrekend:** `open` (dropdown/listbox mode), `textEntry` (allow custom text), `commitOn` (blur/select)
- **Annotatie:** `// XFA Spec 3.3 §17 "choiceList" (p653) — Attributes: open, textEntry, commitOn, use, usehref. We parse: FieldKind::Dropdown. Missing: [open, textEntry, commitOn, use, usehref].`

### `color` (p657–658)
**XFA Spec 3.3 §17 "color"**
- **Attributen:** `cSpace`, `value`, `exData`
- **Wij behandelen:** `value` via `parse_xfa_color` (regel 729) — parsed as "r,g,b"
- **Ontbrekend:** `cSpace` (color space, default "RGB"), `exData` (embedded data)
- **Annotatie:** `// XFA Spec 3.3 §17 "color" (p657) — Attributes: cSpace, value, exData. We parse: [value="r,g,b"]. Missing: [cSpace, exData].`

### `comb` (p659)
**XFA Spec 3.3 §17 "comb"**
- **Attributen:** `maxChars`
- **Wij behandelen:** ❌ Niets — `comb` wordt genegeerd in `add_children`
- **Ontbrekend:** `maxChars` — bepaalt aantal segments in password field
- **Notitie:** Heeft invloed op field rendering (password met comb style)

### `command` (p660–662)
**XFA Spec 3.3 §17 "command"**
- **Attributen:** `connectionName`, `queryString`, `timeout`, `ignoreErrors`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor database queries — niet relevant voor pure template parsing

### `connect` (p663–664)
**XFA Spec 3.3 §17 "connect"**
- **Attributen:** `connectionName`, `relation`, `ref`, `use`, `usehref`, `formatters`
- **Wij behandelen:** ❌ Niets substantieels — niet geparsed
- **Ontbrekend:** Alle attributen
- **Notitie:** Voor data binding connection info — valt onder `<bind>`

### `contentArea` (p665)
**XFA Spec 3.3 §17 "contentArea"**
- **Attributen:** `name`, `x`, `y`, `w`, `h`, `relevant`, `presence`, `use`, `usehref`
- **Wij behandelen:** ✅ Volledig — `read_content_areas` (regel 1554)

### `corner` (p666–667)
**XFA Spec 3.3 §17 "corner"**
- **Attributen:** `radius`, `joinStyle`, `stroke`, `presence`
- **Wij behandelen:** `radius` via `parse_node_style` (regel 712)
- **Ontbrekend:** `joinStyle`, `stroke`, `presence` op corner
- **Annotatie:** `// XFA Spec 3.3 §17 "corner" (p666) — Attributes: radius, joinStyle, stroke, presence. We parse: [radius]. Missing: [joinStyle, stroke, presence].`

### `date` (p668–670)
**XFA Spec 3.3 §17 "date"**
- **Attributen:** `datePattern`, `use`, `usehref`, `editValue`
- **Wij behandelen:** Content via `extract_value_text`
- **Ontbrekend:** `datePattern`, `use`, `usehref`, `editValue`
- **Notitie:** Lage prioriteit — date patterns zijn complex

### `dateTime` (p671–672)
**XFA Spec 3.3 §17 "dateTime"**
- **Attributen:** `datePattern`, `use`, `usehref`, `editValue`
- **Wij behandelen:** Content via `extract_value_text`
- **Ontbrekend:** `datePattern`, `use`, `usehref`, `editValue`
- **Notitie:** Zie date

### `dateTimeEdit` (p673–676)
**XFA Spec 3.3 §17 "dateTimeEdit"**
- **Attributen:** `pickerFormat`, `use`, `usehref`
- **Wij behandelen:** `FieldKind::DateTimePicker` gedetecteerd (regel 797)
- **Ontbrekend:** `pickerFormat`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "dateTimeEdit" (p673) — Attributes: pickerFormat, use, usehref. We parse: FieldKind::DateTimePicker. Missing: [pickerFormat, use, usehref].`

### `decimal` (p677–678)
**XFA Spec 3.3 §17 "decimal"**
- **Attributen:** `fracDigits`, `leadDigits`, `symbol`, `use`, `usehref`
- **Wij behandelen:** Content via `extract_value_text`
- **Ontbrekend:** `fracDigits`, `leadDigits`, `symbol`, `use`, `usehref`

### `defaultUi` (p679)
**XFA Spec 3.3 §17 "defaultUi"**
- **Attributen:** `use`, `usehref`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Default UI voorisso signature — lage prioriteit

### `desc` (p680)
**XFA Spec 3.3 §17 "desc"**
- **Wij behandelen:** ✅ Wordt genegeerd in `add_children` (regel 1415) — is description only
- **Notitie:** Geen actie nodig

### `digestMethod` (p681)
**XFA Spec 3.3 §17 "digestMethod"**
- **Attributen:** `method`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor signatures — lage prioriteit

### `digestMethods` (p682)
**XFA Spec 3.3 §17 "digestMethods"**
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor signatures

### `draw` (p683–690)
**XFA Spec 3.3 §17 "draw"**
- **Attributen:** `name`, `x`, `y`, `w`, `h`, `rotate`, `rawContent`, `accessibleCaption`, `presence`, `relevant`, `use`, `usehref`, `colSpan`, `hAlign`, `vAlign`
- **Wij behandelen:** `name`, box model via `parse_draw`; `value` content (text, image, line, rectangle, arc); `font`, `border`, `fill`, `margin`, `para`
- **Ontbrekend:** `rotate`, `rawContent`, `accessibleCaption`, `colSpan`, `hAlign`, `vAlign`
- **Annotatie:** `// XFA Spec 3.3 §17 "draw" (p683) — Attributes: name, x, y, w, h, rotate, rawContent, accessibleCaption, presence, relevant, use, usehref, colSpan, hAlign, vAlign. We parse: [name, x, y, w, h, value/text|image|line|rect|arc, font, border, fill, margin, para]. Missing: [rotate, rawContent, accessibleCaption, colSpan, hAlign, vAlign].`

### `edge` (p691–692)
**XFA Spec 3.3 §17 "edge"**
- **Attributen:** `stroke`, `cap`, `joinStyle`, `thickness`, `color`, `presence`
- **Wij behandelen:** `stroke`, `thickness`, `color` via `parse_node_style` (regel 586–595)
- **Ontbrekend:** `cap`, `joinStyle`, `presence`
- **Annotatie:** `// XFA Spec 3.3 §17 "edge" (p691) — Attributes: stroke, cap, joinStyle, thickness, color, presence. We parse: [stroke, thickness, color]. Missing: [cap, joinStyle, presence].`

### `encoding` (p693)
**XFA Spec 3.3 §17 "encoding"**
- **Attributen:** `encodingName`, `desc`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor character encoding info — zelden gebruikt in moderne XFA

### `encodings` (p694)
**XFA Spec 3.3 §17 "encodings"**
- **Wij behandelen:** ❌ Niets

### `encrypt` (p695–696)
**XFA Spec 3.3 §17 "encrypt"**
- **Attributen:** `encryptName`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor document encryption — niet relevant voor template parsing

### `event` (p697)
**XFA Spec 3.3 §17 "event"**
- **Attributen:** `name`, `ref`, `activity`, `xfa:contentType`, `level`, `use`, `usehref`
- **Children:** `script`+
- **Wij behandelen:** `activity`, `ref`, `runAt` (via `<script runAt>`) via `collect_event_scripts` (regel 881)
- **Ontbrekend:** `name`, `xfa:contentType`, `level`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "event" (p697) — Attributes: name, ref, activity, xfa:contentType, level, use, usehref. We parse: [activity, ref, runAt via script]. Missing: [name, xfa:contentType, level, use, usehref].`

### `exData` (p698)
**XFA Spec 3.3 §17 "exData"**
- **Attributen:** `contentType`, `maxLength`, `rid`, `href`, `excludeDesc`, `use`, `usehref`
- **Wij behandelen:** `contentType` via `extract_value_text` (text/html stripping) en `extract_exdata_font_size`
- **Ontbrekend:** `maxLength`, `rid`, `href`, `excludeDesc`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "exData" (p698) — Attributes: contentType, maxLength, rid, href, excludeDesc, use, usehref. We parse: [contentType for HTML stripping]. Missing: [maxLength, rid, href, excludeDesc, use, usehref].`

### `exObject` (p699)
**XFA Spec 3.3 §17 "exObject"**
- **Attributen:** `className`, `contentType`, `href`, `name`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor external objects — zelden gebruikt

### `exclGroup` (p700–706)
**XFA Spec 3.3 §17 "exclGroup"**
- **Attributen:** `name`, `layout`, `x`, `y`, `w`, `h`, `minH`, `minW`, `maxH`, `maxW`, `extra`, `relevant`, `use`, `usehref`, `access`, `accessKey`, `colSpan`, `hAlign`, `vAlign`, `presence`, `anchorType`
- **Wij behandelen:** ✅ Volledig — `parse_subform_node` + `add_children`; `GroupKind::ExclusiveChoice` (regel 463)
- **Notitie:** Children (checkButton/radio) worden omgezet naar `FieldKind::Radio` (regel 93)

### `execute` (p707)
**XFA Spec 3.3 §17 "execute"**
- **Attributen:** `runAt`, `use`, `usehref`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor server-side execute — niet relevant voor lokale parsing

### `extras` (p708)
**XFA Spec 3.3 §17 "extras"**
- **Attributen:** `突` (custom), `when`, `label`, `use`, `usehref`
- **Wij behandelen:** ⚠️ Wordt genegeerd in `add_children`
- **Notitie:** Custom attributes voor extensibility — lage prioriteit

### `field` (p709–714)
**XFA Spec 3.3 §17 "field"**
- **Attributen:** `name`, `x`, `y`, `w`, `h`, `minH`, `minW`, `maxH`, `maxW`, `extra`, `relevant`, `use`, `usehref`, `access`, `accessKey`, `anchorType`, `colSpan`, `hAlign`, `vAlign`, `presence`, `rotate`, `locale`
- **Wij behandelen:** `name`, box model via `parse_field`; `occur`, `caption`, `ui`, `value`, `items`, `font`, `border`, `margin`, `para`, `calculate`, `validate`, `event`, `bind`
- **Ontbrekend:** `access`, `accessKey`, `anchorType`, `hAlign`, `vAlign`, `rotate`, `locale`, `relevant`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "field" (p709) — Attributes: access, accessKey, anchorType, colSpan, h, locale, maxH, maxW, minH, minW, name, presence, relevant, rotate, use, usehref, w, x, y, hAlign, vAlign, layout. We parse: [name, x, y, w, h, minH, minW, maxH, maxW, layout, occur, caption, ui, value, items, font, border, margin, para, calculate, validate, event, bind]. Missing: [access, accessKey, anchorType, hAlign, vAlign, rotate, locale, relevant, use, usehref].`

### `fill` (p715–718)
**XFA Spec 3.3 §17 "fill"**
- **Attributen:** `presence`, `relevant`, `use`, `usehref`
- **Children:** `color`, `solid`, `linear`, `radial`, `pattern`, `stipple`, `exData`
- **Wij behandelen:** `<color>` en `<solid><color>` voor bg_color via `parse_node_style` (regel 541)
- **Ontbrekend:** `linear` (gradient), `radial` (radial gradient), `pattern` (pattern fill), `stipple` (stipple fill), `presence`, `relevant`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "fill" (p715) — Attributes: presence, relevant, use, usehref. Children: color, solid, linear, radial, pattern, stipple, exData. We parse: [color, solid>color for bg_color]. Missing: [linear, radial, pattern, stipple, presence, relevant, use, usehref].`

### `filter` (p719)
**XFA Spec 3.3 §17 "filter"**
- **Attributen:** `dataFilter`, `logic`
- **Wij behandelen:** ❌ Niets
- **Notitie:** Voor data filtering in connection elements

### `float` (p720–721)
**XFA Spec 3.3 §17 "float"**
- **Attributen:** `dataPrecision`, `symbol`, `use`, `usehref`
- **Wij behandelen:** Content via `extract_value_text`
- **Ontbrekend:** `dataPrecision`, `symbol`, `use`, `usehref`

### `font` (p722–728)
**XFA Spec 3.3 §17 "font"**
- **Attributen:** `typeface`, `size`, `weight`, `posture`, `fontType`, `purpose`, `encoding`, `glyphlist`, `actual`, `actualType`, `embed`, `status`, `color`, `fontHorizontalScale`, `letterSpacing`, `letterSpacing`
- **Wij behandelen:** `typeface`, `size`, `weight`, `posture`, `color` (attr + fill), `fontHorizontalScale`, `letterSpacing`
- **Ontbrekend:** `fontType`, `purpose`, `encoding`, `glyphlist`, `actual`, `actualType`, `embed`, `status`
- **Annotatie:** `// XFA Spec 3.3 §17 "font" (p722) — Attributes: typeface, size, weight, posture, fontType, purpose, encoding, glyphlist, actual, actualType, embed, status, color, fontHorizontalScale, letterSpacing. We parse: [typeface, size, weight, posture, color, fontHorizontalScale, letterSpacing]. Missing: [fontType, purpose, encoding, glyphlist, actual, actualType, embed, status].`

### `format` (p729–734)
**XFA Spec 3.3 §17 "format"**
- **Attributen:** `category`, `crack`, `custom`, `datePattern`, `digitSet`, `fracDigits`, `leadDigits`, `locales`, `picture`, `symbol`, `timezone`, `use`, `usehref`
- **Wij behandelen:** `category` en `crack` via `<validate>` handling (niet volledig geïmplementeerd)
- **Ontbrekend:** `picture` (XFA picture pattern syntax — belangrijk!), `custom`, `datePattern`, `digitSet`, `fracDigits`, `leadDigits`, `locales`, `symbol`, `timezone`, `use`, `usehref`
- **Annotatie:** `// XFA Spec 3.3 §17 "format" (p729) — Attributes: category, crack, custom, datePattern, digitSet, fracDigits, leadDigits, locales, picture, symbol, timezone, use, usehref. We parse: [category via validate]. Missing: [picture (CRITICAL - XFA picture clause), custom, datePattern, digitSet, fracDigits, leadDigits, locales, symbol, timezone, use, usehref].`

---

## Kritische Gaps (Impact op Rendering)

1. **`picture` attribute in `<format>`** — XFA picture clauses worden veel gebruikt voor number/date/time formatting. Zonder dit werken geformatteerde velden niet correct.

2. **`hAlign`/`vAlign` op `field` en `draw`** — Text alignment in velden is een basis feature die ontbreekt.

3. **`rotate` op `draw` en `field`** — Rotated text/images worden niet gerespecteerd.

4. **`colSpan` op `field` en `draw`** — Voor tabel-layout subforms is column spanning essentieel.

5. **`fontHorizontalScale` en `letterSpacing`** — Wel geïmplementeerd maar alleen via `<font>`, niet via `<para>`.

6. **Gradient fills (`linear`, `radial`)** — Background gradients worden niet gerenderd.

---

## Aangebrachte Annotaties

Geen code-wijzigingen gemaakt tijdens deze review — alleen annotaties gedocumenteerd.

---

## Aanbevelingen

1. **Hoge prioriteit:** Implementeer `picture` pattern parsing voor `format` element (XFA 3.3 §17.5.4)
2. **Hoge prioriteit:** Parse `hAlign`/`vAlign` op `field` en `draw`
3. **Middel prioriteit:** Implementeer `rotate` attribute
4. **Middel prioriteit:** Implementeer `colSpan` voor field/draw in table layout
5. **Lage prioriteit:** Gradient fills, barcode attributes, certificate/signatures

---

## Verificatie Commando's

```bash
# Tests
cargo test -p pdf-xfa

# Clippy
cargo clippy -p pdf-xfa -- -D warnings
```
