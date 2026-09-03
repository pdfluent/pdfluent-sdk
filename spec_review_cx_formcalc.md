# FormCalc Chapter 25 Review

Scope: XFA Spec 3.3 Chapter 25, pages 1049-1149, reviewed against `crates/formcalc-interpreter`.

Reviewed files:
- `crates/formcalc-interpreter/src/lexer.rs`
- `crates/formcalc-interpreter/src/parser.rs`
- `crates/formcalc-interpreter/src/interpreter.rs`
- `crates/formcalc-interpreter/src/builtins.rs`
- `crates/formcalc-interpreter/src/ast.rs`
- `crates/formcalc-interpreter/src/value.rs`
- `crates/formcalc-interpreter/src/som_bridge.rs`

Verification:
- `cargo test -p formcalc-interpreter` with `CARGO_TARGET_DIR=<workspace>/target-cx`
- `cargo clippy -p formcalc-interpreter --all-targets -- -D warnings` with the same target dir

## BUILT-IN FUNCTIES MATRIX

### §25.3 Arithmetic Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| Abs | p1081 | Yes | Yes | Yes | Null-handling gecorrigeerd. |
| Avg | p1082 | Yes | Yes | Yes | Negeert null-waarden en retourneert null als alles null is. |
| Ceil | p1083 | Yes | Yes | Yes | Null-handling gecorrigeerd. |
| Count | p1084 | Yes | Yes | Yes | Telt nu alleen non-null argumenten conform spec. |
| Floor | p1085 | Yes | Yes | Yes | Null-handling gecorrigeerd. |
| Max | p1086 | Yes | Yes | Yes | Negeert null-waarden; null als alles null is. |
| Min | p1087 | Yes | Yes | Yes | Negeert null-waarden; null als alles null is. |
| Mod | p1088 | Yes | Yes | Mostly | Null-handling en divide-by-zero correct; floating remainder volgt Rust `%`, wat de spec-sign-regel volgt. |
| Round | p1089 | Yes | Yes | Mostly | Null-handling, default decimals en precision clamp naar 12 toegevoegd; negatieve precision wordt nu naar 0 geklemd. |
| Sum | p1090 | Yes | Yes | Yes | Negeert null-waarden; null als alles null is. |

### §25.4 Date and Time Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| Date | p1091 | Partial | Yes | Partial | Gebruikt huidige UTC-datum; prevailing locale/system-local date is nog TODO. |
| Date2Num | p1092 | Partial | Mostly | Partial | Null-handling en return-0 bij parse failure correct; picture clause en locale support zijn beperkt tot eenvoudige numerieke/Engelse maandformaten. |
| DateFmt | p1093 | Partial | Yes | Partial | Stijlmapping aanwezig; locale-argument wordt nog genegeerd. |
| IsoDate2Num | p1094 | Partial | Yes | Partial | Ondersteunt canonieke ISO datum/date-time varianten; validatie is nog basic. |
| IsoTime2Num | p1095 | Partial | Yes | Partial | Ondersteunt gangbare ISO tijden en offsets; default “current timezone” gedrag is nog niet volledig. |
| LocalDateFmt | p1096 | Partial | Yes | Partial | Momenteel alias van `DateFmt`; echte gelokaliseerde promptsymbolen ontbreken. |
| LocalTimeFmt | p1097 | Partial | Yes | Partial | Momenteel alias van `TimeFmt`; echte gelokaliseerde promptsymbolen ontbreken. |
| Num2Date | p1098 | Partial | Mostly | Partial | Null-handling en empty-string bij invalid day toegevoegd; formatting ondersteunt basis picture tokens, niet volledige locale-set. |
| Num2GMTime | p1099 | Partial | Mostly | Partial | Bestaat; gebruikt nu dezelfde formatter als `Num2Time`, zonder volledige GMT/locale picture semantics. |
| Num2Time | p1100 | Partial | Mostly | Partial | 1-based millisecond epoch gecorrigeerd; formatting ondersteunt basis `HH:MM:SS`/AM-PM/GMT patronen. |
| Time | p1101 | Yes | Yes | Mostly | Retourneert huidige UTC millisecond-of-day + 1, conform epoch; locale/system presentation is niet relevant. |
| Time2Num | p1102 | Partial | Mostly | Partial | 1-based epoch gecorrigeerd; parse van offsets en AM/PM aanwezig, maar format/locale-interpretatie is nog beperkt. |
| TimeFmt | p1104 | Partial | Yes | Partial | Basale stijlmapping aanwezig; locale-argument genegeerd. |

### §25.5 Financial Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| Apr | p1105 | Yes | Yes | Mostly | Argumentvolgorde en positieve-validatie gecorrigeerd; numerieke solver blijft benaderend. |
| CTerm | p1106 | Yes | Yes | Yes | Valideert nu non-positive invoer als error i.p.v. stil 0 terug te geven. |
| FV | p1107 | Yes | Yes | Yes | Positieve-validatie toegevoegd; 0-rate pad behouden. |
| IPmt | p1108 | Yes | Yes | Mostly | Annual-rate -> monthly-rate en `first month + month count` semantiek gecorrigeerd. |
| NPV | p1109 | Yes | Yes | Yes | Positieve discount-rate validatie toegevoegd. |
| Pmt | p1110 | Yes | Yes | Yes | Positieve-validatie toegevoegd. |
| PPmt | p1111 | Yes | Yes | Mostly | Annual-rate -> monthly-rate en payment-vs-interest validatie gecorrigeerd. |
| PV | p1112 | Yes | Yes | Yes | Positieve-validatie toegevoegd. |
| Rate | p1113 | Yes | Yes | Yes | Positieve-validatie toegevoegd. |
| Term | p1114 | Yes | Yes | Yes | Positieve-validatie toegevoegd. |

### §25.6 Logical Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| Choose | p1115 | Yes | Yes | Yes | `n1` null -> null; out-of-range -> empty string conform spec. |
| Exists | p1116 | Partial | Partial | Partial | Werkt voor simpele accessors en string-path SOM extensies; full reference/accessor grammar ontbreekt. |
| HasValue | p1117 | Partial | Mostly | Partial | Blank/whitespace gedrag gecorrigeerd; full accessor/reference semantics ontbreken nog. |
| Oneof | p1118 | Yes | Yes | Yes | Werkt inclusief null-comparisons via `Value` equality. |
| Within | p1119 | Yes | Yes | Mostly | Ondersteunt numeric en lexicographic pad; locale-sensitive collation ontbreekt. |

### §25.7 String Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| At | p1120 | Yes | Yes | Yes | Null -> null; empty needle -> 1; not found -> 0. |
| Concat | p1122 | Yes | Yes | Yes | Null-argumenten worden `""`; all-null -> null. |
| Decode | p1123 | TODO | TODO | TODO | Stub toegevoegd met expliciete spec-TODO runtime error. |
| Encode | p1124 | TODO | TODO | TODO | Stub toegevoegd met expliciete spec-TODO runtime error. |
| Format | p1125 | TODO | TODO | TODO | Stub toegevoegd; picture clause engine ontbreekt. |
| Left | p1127 | Yes | Yes | Yes | `n<=0` -> `""`, overrun -> hele string. |
| Len | p1128 | Yes | Yes | Yes | Null -> null; telt nu Unicode scalar chars i.p.v. bytes. |
| Lower | p1129 | Partial | Mostly | Partial | 2e locale-argument geaccepteerd maar genegeerd. |
| Ltrim | p1130 | Yes | Yes | Mostly | Trimt leading Unicode/Rust whitespace; spec Zs-set niet apart getest. |
| Parse | p1131 | TODO | TODO | TODO | Stub toegevoegd; picture clause parser ontbreekt. |
| Replace | p1132 | Yes | Yes | Yes | 2- of 3-arg variant; omitted/null replacement -> empty string. |
| Right | p1133 | Yes | Yes | Yes | `n<=0` -> `""`, overrun -> hele string. |
| Rtrim | p1134 | Yes | Yes | Mostly | Trimt trailing Unicode/Rust whitespace; spec Zs-set niet apart getest. |
| Space | p1135 | Yes | Yes | Yes | Null -> null; negative -> 0 spaces. |
| Str | p1136 | Yes | Yes | Mostly | Fixed-width formatting en `*` overflow aanwezig; volledige spec-rounding/padding is functioneel maar niet exhaustief getest. |
| Stuff | p1137 | Yes | Yes | Yes | 3/4-arg variant; start/delete bounds gecorrigeerd. |
| Substr | p1138 | Yes | Yes | Yes | Start en length bounds conform spec-normalisatie. |
| Uuid | p1139 | Yes | Mostly | Mostly | Default 32 hex chars; `Uuid(1)` dashed. Niet cryptografisch of spec-garantie-hard. |
| Upper | p1140 | Partial | Mostly | Partial | 2e locale-argument geaccepteerd maar genegeerd. |
| WordNum | p1141 | Partial | Mostly | Partial | Simple/monetary opties 0/1/2 aanwezig, English-only; locale-argument genegeerd. |

### §25.8 URL Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| Get | p1143 | TODO | TODO | TODO | Stub toegevoegd; SOM dispatcher kapt nu alleen SOM-paden af, niet URL’s. |
| Post | p1144 | TODO | TODO | TODO | Stub toegevoegd. |
| Put | p1146 | TODO | TODO | TODO | Stub toegevoegd. |

### §25.9 Miscellaneous Built-in Functions

| Functie | Spec pagina | Geïmplementeerd? | Parameters correct? | Edge cases? | Notities |
| --- | --- | --- | --- | --- | --- |
| Eval | n/a in pp1049-1149 | TODO | TODO | TODO | Niet teruggevonden in de geëxtraheerde Chapter 25 range; stub toegevoegd op verzoek. |
| Null | p1057 (literal) | Yes | Yes | Yes | `null` literal en `Null()` compat-functie werken beide. |
| Ref | p1147 | TODO | TODO | TODO | Stub toegevoegd; volledige reference/value dual semantics ontbreken nog. |
| UnitValue | p1148 | Yes | Yes | Mostly | Basis unitspan parsing en conversie voor `in/mm/cm/pt/mp`. |
| UnitType | p1149 | Yes | Yes | Mostly | Canonical unit name mapping voor basis unitspans. |

## Grammar Compliance Checklist

### §25.1 Grammar and Syntax

- `Done`: `;` en `//` comments.
- `Done`: CR/LF line terminators worden nu als statement separators getokenized.
- `Done`: identifiers zijn case-sensitive; starts met letter, `_`, `$`, `!`.
- `Done`: string literals ondersteunen doubled quotes en Unicode `\u`/`\U` escapes.
- `Partial`: number literals ondersteunen integer/decimal/exponent; `nan`/`inf` literals zijn nog niet expliciet ondersteund.
- `Done`: symbolic en mnemonic operators `=`, `|`/`or`, `&`/`and`, `==`/`<>`/`eq`/`ne`, `< <= > >=` en `lt/le/gt/ge`, `+ - * /`, `not`.
- `Done`: unary plus toegevoegd.
- `Done`: `&` betekent nu logical-and conform spec; string concatenatie verloopt via `Concat(...)`.
- `Done`: expression lists, assignments, if/elseif/else, while, for upto/downto/step, function calls, user-defined functions.
- `Partial`: `func ... do ... endfunc` wordt ondersteund, maar de interpreter laat de oude newline-vorm nog impliciet toe.
- `Partial`: `foreach x in (a, b, c) do ... endfor` werkt voor argument lists; accessor-set semantics zijn nog beperkt.
- `Partial`: if zonder else retourneert nu `0`; loop default-resultaten zijn naar `0` getrokken, maar `break`/`continue` expression values zijn niet als first-class runtime waarden gemodelleerd.
- `Done`: built-in functions blijven precedence houden boven user-defined functions.
- `Partial`: boolean coercion volgt nu numerieke coercion zoals spec; null arithmetic/logical corner cases in evaluator zijn gecorrigeerd.
- `Partial`: simple member access en dotted method names werken.
- `TODO`: accessors met `[n]`, `[*]`, `[+n]`, `[-n]`, `..`, `.#`, `.*`, `this` en volledige reference-semantiek.
- `TODO`: expliciete block expression `do ... end`.
- `TODO`: `throw` / `exit` grammar.

### §25.2 FormCalc Support for Locale

- `Partial`: default date/time format helpers toegevoegd.
- `TODO`: prevailing locale resolution chain.
- `TODO`: locale-specific picture clause parsing/formatting.
- `TODO`: localized `LocalDateFmt` / `LocalTimeFmt` symbol sets.
- `TODO`: locale-sensitive string collation/casing beyond Rust default Unicode casing.

## Lijst van ontbrekende functies

Volledig nog niet geïmplementeerd, maar nu wel expliciet gemarkeerd als spec-TODO runtime stubs:
- `Decode`
- `Encode`
- `Format`
- `Parse`
- `Get`
- `Post`
- `Put`
- `Eval`
- `Ref`

Niet volledig conform spec, maar gedeeltelijk aanwezig:
- `Date`
- `Date2Num`
- `DateFmt`
- `IsoDate2Num`
- `IsoTime2Num`
- `LocalDateFmt`
- `LocalTimeFmt`
- `Num2Date`
- `Num2GMTime`
- `Num2Time`
- `Time2Num`
- `TimeFmt`
- `Exists`
- `HasValue`
- `Lower`
- `Upper`
- `WordNum`
- `Uuid`

## Lijst van alle aangebrachte annotaties, fixes, en TODO's

- Annotaties toegevoegd in `builtins.rs` voor spec-secties zoals `Abs`, `Avg`, `Concat`, `Str`, en voor overlappende SOM-vs-URL builtins in `som_bridge.rs`.
- `value.rs`: boolean coercion gecorrigeerd naar numerieke coercion conform §25.1; `is_blankish()` toegevoegd voor `HasValue`.
- `lexer.rs`: CR/LF line terminators, `!`-identifiers, Unicode string escapes, en spec-whitespace verbeterd.
- `parser.rs`: unary plus toegevoegd; `&` verschoven naar logical-and; `for var` en `func ... do` ondersteund; `foreach (...)` argument lists toegevoegd.
- `interpreter.rs`: null-aware unary/binary semantics verbeterd; if-zonder-else retourneert `0`; simpele accessor-aware `Exists`/`HasValue` toegevoegd; SOM/string-path dispatch opgeschoond.
- `som_bridge.rs`: SOM builtins met overlappende naam (`Get`, `Exists`) worden alleen nog onderschept als het eerste argument echt op een SOM-path lijkt.
- `builtins.rs`: arithmetic builtins spec-conform gemaakt voor null-handling en non-null aggregation.
- `builtins.rs`: string builtins gecorrigeerd voor null/empty bounds, optional parameters, en fixed-width `Str`.
- `builtins.rs`: 1-based time epoch gecorrigeerd in `Time`, `Time2Num`, `Num2Time`.
- `builtins.rs`: eenvoudige default format helpers toegevoegd (`DateFmt`, `TimeFmt`, `LocalDateFmt`, `LocalTimeFmt`).
- `builtins.rs`: basis ISO/timezone parsing toegevoegd voor `IsoDate2Num`, `IsoTime2Num`, `Time2Num`.
- `builtins.rs`: financiële functies gevalideerd en waar nodig semantisch gecorrigeerd, vooral `Apr`, `IPmt`, `PPmt`.
- `builtins.rs`: `UnitValue` en `UnitType` toegevoegd.
- `builtins.rs`: TODO stubs toegevoegd voor ontbrekende built-ins zodat ontbrekende spec-dekking nu expliciet is in plaats van een stille `UnknownFunction`.
- Tests aangepast zodat ze Chapter 25 beter weerspiegelen: `Concat(...)` i.p.v. infix `&` voor strings, 1-based time epoch, en `Uuid()` default zonder dashes.

## Belangrijkste open gaps na deze review

- Volledige accessor/reference grammar en runtime model (`Ref`, `[*]`, indexed accessors, `..`, `.#`, `.*`, reference comparisons).
- Volledige locale/picture clause engine voor date/time/string formatting en parsing.
- URL built-ins met echte protocol-host integratie.
- `Decode`, `Encode`, `Format`, `Parse`, `Eval`.
