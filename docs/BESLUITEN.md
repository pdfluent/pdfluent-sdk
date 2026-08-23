# Besluiten

Voor keuzes die niet bij één bestand horen. Hoort een keuze wél bij één plek in
de code, dan staat de uitleg dáár — in het doc-commentaar boven het ding zelf,
niet hier en niet in een commitboodschap.

Waarom dit bestand bestaat: op 22-08 kostte het twee uur om te achterhalen
waarom 883 regels PDF/A-reparatiecode nergens wordt aangeroepen. Het antwoord
bestond, in een commitboodschap, op een tak die nog niet binnen was. Je moest
weten dat je moest zoeken, en waar.

**Vorm.** Per besluit: wat, waarom, wat er gemeten is, en wanneer we het
zouden heroverwegen. Vijf regels is genoeg. Nieuwste bovenaan.

---

## 22-08-2026 · De acht ongebruikte PDF/A-passen blijven ongebruikt

**Wat.** Acht publieke functies in `pdfa_fonts.rs` (883 regels) worden door niets
aangeroepen en dat blijft zo. Niet weggooien, niet aansluiten.

**Waarom.** Aansluiten maakt het slechter. Gemeten met veraPDF over 40 documenten
per pas: geen enkel oordeel verbeterde, en drie van de vier maakten conforme
documenten niet-conform. `fix_embedded_font_metrics` alleen al brak 6 van 40.

**Wat we leerden.** De eigen teller van een reparatiepas zegt niets over zijn
waarde. `fix_type1_widths` meldt werk op 200 van 200 documenten en convergeert —
dat leest als een gat en was schade.

**Heroverwegen.** Als iemand ze aanroept op een ándere plek in de pijplijn dan
het einde. De meting is gedaan ná de omzetting; een pas kan daar schadelijk zijn
en eerder zinvol. Reproduceer met `examples/unwired_pass_probe.rs`.

## 22-08-2026 · De 300-steekproef is een hek, de holdout is de claim

**Wat.** `corpus:pdfa-conformance` (300 documenten) blijft het regressiehek. Elk
cijfer dat naar buiten gaat komt van `govdocs_holdout_1000.txt`.

**Waarom.** Vijf reparatierondes zijn gedreven door precies die 300 mislukkingen,
en de steekproef staat inmiddels op 300/300. Een vaste set waar je tegenaan werkt
loopt naar 100% en beschrijft dan alleen nog hoe goed je die set kent.

**Gemeten.** Holdout 05-08: 991/1000. Opnieuw 20-08: 981/1000, nadat veraPDF
stopte met documenten die het niet kon doorlezen als geslaagd te tellen.

**Heroverwegen.** Zodra er op de holdout gediagnosticeerd wordt. Dan is hij geen
holdout meer en moet er een nieuwe getrokken worden.

## OPEN · Gaat fontfallback standaard aan bij tekstvervanging?

**Gemeten 22-08, en het cijfer is er nu.** De job die dit moest beantwoorden had
nooit gedraaid: hij gaf een vlag mee die het script niet kende en viel binnen 0,4
seconde om. Gerepareerd, en toen liep hij in drie minuten.

| op 196 bruikbare documenten | `Deny` (nu) | `InjectStandard` |
|---|---|---|
| vervangen | 180 (91,8%) | **193 (98,5%)** |
| daarna uitleesbaar | 171 (87,2%) | **184 (93,9%)** |

Alle dertien die op de fallback strandden slagen ermee, inclusief de drie subsets
zonder bekende codering waarvan ik verwachtte dat ze zouden blijven falen. De
negen die na vervanging onleesbaar blijven zijn exact dezelfde negen — fallback
raakt die niet, wat bevestigt dat het daar om `/ToUnicode` gaat.


**Wat.** `FontFallback::Deny` is nu de standaard: kan een lettertype een teken
niet schrijven, dan weigert de vervanging. Het alternatief voegt een
Helvetica-hulpbron toe.

**Wat het kost.** 13 van de 20 mislukkingen op de corpussteekproef zijn puur deze
instelling. Tien daarvan zijn "teken ontbreekt" en zouden met fallback slagen.

**Wat pleit vóór aanzetten.** Een ontwikkelaar die de SDK evalueert op eigen
PDF's ziet nu één op de vijftien falen. Dat is de duurste minuut in de
verkoopcyclus.

**Wat het niet is.** Een stille wijziging: het resultaat meldt per bewerking
`font_substituted`. De bibliotheek was daar al op ontworpen.

**Wacht op.** Jasper.

## OPEN · Wordt er gepubliceerd?

**Wat er in master zit en niet in het pakket dat mensen installeren:**

- Een pagina verloor al haar tekst na de eerste backslash die erin stond. Dat
  raakt elk document waarvan de tekst een `\` bevat — pdfTeX schrijft die als
  `(\x00\\\\)`. veraPDF noemde het resultaat conform, dus geen validatie ving
  het.
- Spaties werden als `)` getekend zodra een subset-lettertype zijn spatie-glyph
  zonder omtrek had. Zichtbaar op de pagina, niet alleen in de tekstlaag.
- Tekencodes onder 32 werden blanco gemaakt, ook waar `/Differences` ze een
  letter geeft. Raakt TeX-documenten.
- `sign_pdf_incremental` werkte bij geen enkel document.
- Een gecertificeerde handtekening meldde nooit zijn eigen `/DocMDP`-niveau, dus
  elke "mag deze wijziging?"-beslissing behandelde hem als ongecertificeerd.

**Wat het meten betreft.** Zolang dit niet gepubliceerd is, staan de twee assen
op verschillende builds: de conformiteit die een buitenstaander kan nadraaien
komt uit beta.17.5, het tekstbehoud uit de broncode. Eén publicatie zet ze op
dezelfde build en maakt `benchmarks/pdfa/reproduce/` een volledig antwoord in
plaats van een half antwoord.

**Waarom dit niet zelf gebeurt.** Publiceren is een stap naar buiten, en
onomkeerbaar zodra een registry het pakket heeft.

**Wat er klaarligt.** De reparaties zitten in master met tests die in CI draaien.
`benchmarks/pdfa/reproduce/package.json` pint de versie; na publicatie hoeft daar
één versienummer in en dan draait dezelfde meting op het nieuwe pakket.

**Wacht op.** Jasper.

## OPEN · Verschuift de grens tussen open en propriëtair? (!16)

**Wat.** !16 maakt `pdf-compliance` een zuivere validator door generatiecode naar
`pdf-manip` te verplaatsen.

**Stand.** Gerebased, gerepareerd (het verwijderde 543 regels PDF/UA-generatie in
plaats van ze te verplaatsen) en groen. Niet gemerged.

**Waarom het wacht.** Dit verschuift wat er open en wat er propriëtair is. Dat is
geen technische afweging.

**Wacht op.** Jasper.

## 22-08 — Voorbeeldprogramma's meten niet automatisch wat we uitleveren

`examples/convert_pdfa.rs` roept de reparatiestappen los aan, in een eigen
volgorde. Dat is bruikbaar om te zien wat elke stap doet, en het is het eerste
wat je pakt als je één document wilt uitpluizen. Die volgorde was afgedreven van
`pdfa::convert_bytes`: hij riep nog `strip_control_chars_from_streams` aan met
een lege "bewaren"-verzameling, waardoor elke tekencode onder 32 een spatie werd.
Op een TeX-subset zijn dat de gewone letters.

Ik heb daar een hele avond op gediagnosticeerd — inclusief drie hypotheses over
lettertypen die stuk voor stuk door meting onderuit gingen — en de conclusie
"dit document is vernield, 6,0% tekstbehoud" gepubliceerd in
`benchmarks/PDFA_HOLDOUT_22AUG.md`. Door de echte pijplijn staat hetzelfde
bestand op 100,0% en conform.

**Besluit:** elk cijfer gaat door `examples/pdfa_convert_real.rs`, dat
rechtstreeks `pdfa::convert_bytes` aanroept. Het hand-nagebouwde voorbeeld is
diagnosegereedschap, geen meetinstrument.

**Waarom dit meer is dan één bug:** twee stukken code die hetzelfde horen te doen
en apart onderhouden worden, gaan uit elkaar lopen, en het verschil valt niet op
omdat beide iets plausibels opleveren. De `preserve`-berekening zit daarom nu in
één functie, `control_codes_to_preserve`, met de reden erboven. Vastgelegd in
`crates/pdf-manip/tests/pdfa_control_code_glyphs.rs`, dat meedraait in
`quality:cargo-test`, en getoetst door de fix te breken.


## 22-08 — Eén backslash in de tekst kostte de rest van de pagina

`truncate_long_strings_in_content` (PDF/A-limiet van 32767 bytes per tekstreeks)
bepaalde het einde van een `(...)`-reeks met de regel "de vorige byte is geen
backslash". Dat is niet de regel: een escape verbruikt precies één byte erna, dus
in `(\\)` escapet de backslash zichzelf en sluit de haak wél.

Gevolg: zodra de tekst van een pagina een backslash bevatte — `(\x00\\\\)` in
pdfTeX-uitvoer — liep de scan door tot het einde van de contentstroom, kwam
boven de 32767 uit, en hield de functie de eerste 32767 bytes over. **Alles
daarna werd weggegooid.** Op `170_170407.pdf` viel pagina 3 van 67.786 naar
36.668 bytes; elke pagina van dat document verloor de tekst na zijn eerste
backslash. veraPDF noemde het resultaat conform.

Twee dingen zijn veranderd:

1. De escape wordt gelezen zoals ISO 32000-1 §7.3.4.2 hem beschrijft.
2. Een reeks waarvan we het einde niet vinden, laten we met rust. Een mogelijk
   te lange reeks laten staan is een validatiebevinding; de pagina afkappen is
   dataverlies. Dat vangnet is wat de schade tegenhoudt als de ontleding
   onverhoopt toch misgaat.

Dezelfde foute regel stond op drie plekken in dit bestand. Ze gebruiken nu één
functie, `end_of_literal_string`, met de reden erboven — anders landt de
volgende correctie weer in twee van de drie. De andere twee scanners
(`collect_xobject_refs_from_form`, `fix_emc_in_bytes`) kapten niets af, maar
werden na de eerste backslash blind voor de rest van de stroom.

Vastgelegd in vier tests in `pdfa_cleanup.rs`, elk getoetst door de bijbehorende
helft van de fix te breken. Let op de eerste poging: mijn eerste testgeval was
te kórt om de schade te laten optreden, dus alle drie bleven groen met de fout
erin. En mijn eerste *mutatie* was ook de verkeerde — ik zette het overslaan van
de escape uit in plaats van de historische regel terug te zetten, en dat gaf
toevallig hetzelfde resultaat. Een test die het gebrek niet kan uitlokken toetst
niets; een mutatie die het gebrek niet nabootst, toetst de test niet.

## 22-08 — Een lege omtrek is geen ontbrekende glyph

`fix_cid_font_notdef` beschouwde elke glyph zonder omtrek als weggesneden en
verving hem. Een spatie hééft geen omtrek. Omdat `glyph_index(' ')` in een
subset vaak niets vindt, viel de vervanging terug op "de laagste glyph die wél
een omtrek heeft" — op `170_170407.pdf` was dat `)`, dus de pagina tekende
"SWAT)SC)Working)Group".

`/ToUnicode` scheidt de twee gevallen: zegt het document zelf dat deze CID een
spatie is, dan is de lege omtrek de juiste. Zie `tounicode_blank_cids`.

## 22-08 — Vier open merge requests, beoordeeld op inhoud

`scripts/ci/mr_staleness.py` meldt ze al op leeftijd. Wat het script niet kan
zeggen is of er nog iets in zit. Nagekeken op 22-08:

| MR | leeftijd | achter | inhoud | zit het in master? |
|---|---|---|---|---|
| !12 | 81d | 329 | StructTree-gestuurde logische tekstextractie, plus gedraaide tekst, overprint-dedup en ligaturen (3.217 regels) | **nee** |
| !9 | 81d | 329 | `XFA_DRAW_LINE_SPAN` — celranden in XFA-tabellen tekenen (1.023 regels) | **nee** |
| !8 | 81d | 498 | `<arc>` renderen (cirkels, ellipsen, zegelringen) plus drie occur-parity-fixes (849 regels) | **nee** |
| !16 | 79d | 10 | `pdf-compliance` als zuivere lezer; de tagged-generator verhuist naar `pdf-manip` (10 regels) | n.v.t. — grensbesluit |

Geen van de drie functies is er langs een andere weg ingekomen; ik heb op de
kenmerkende namen gezocht in `origin/master`. Dit is dus echte, onverzilverde
functionaliteit, geen archief dat zich voordoet als een plan.

**Wat dit kost, per stuk:**

- **!16** is klein en groen, en tien commits achter. De enige vraag is waar de
  grens tussen open en gesloten ligt. Dat is een besluit voor Jasper, geen werk.
- **!8, !9, !12** zijn 329 tot 498 commits achter. Rebasen is hier geen middag
  meer — dat is het werk grotendeels opnieuw doen. Dat is precies waar de regel
  "rebase zolang dat nog een middag is" voor bestaat, en die middag is voorbij.

**Waarom ik ze niet zelf merge:** het zijn productwijzigingen aan de
XFA-weergave en aan tekstextractie, ongereviewd, op een branch die master voedt.
Een van de drie zet bovendien een vlag standaard aan. Dat is niet iets om
ongevraagd binnen te halen omdat een teller op rood staat.

**Wat er wél moet gebeuren:** per stuk merge of sluit. Sluiten laat de branch
bestaan, dus het kost niets behalve de schijn dat er iets in de wachtrij staat —
en bij !8, !9 en !12 is die schijn inmiddels duurder dan het werk zelf, want
niemand weet meer of het nog past.
