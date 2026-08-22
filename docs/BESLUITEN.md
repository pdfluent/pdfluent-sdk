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
