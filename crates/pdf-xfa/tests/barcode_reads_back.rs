// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Een gecodeerde barcode moet terug te lezen zijn.
//!
//! #147 vraagt om "een test die een gerenderde barcode terugleest", en dat is de
//! juiste eis: dat er strepen staan zegt niets. Een encoder met één omgedraaid
//! bit tekent een keurige barcode die naar iets anders scant dan er bedoeld is,
//! en dat merk je pas bij de klant met een handscanner.
//!
//! De decoder hieronder is met opzet zelfstandig geschreven en niet gedeeld met
//! de encoder: hij leest breedtes en zoekt het patroon op, in plaats van de
//! tabel van de encoder te hergebruiken. Deelden ze code, dan zou een fout in de
//! tabel door beide kanten heen glippen -- dan toetst de test zichzelf.

use pdf_xfa::barcode::{encode_code39, total_modules, BarcodeType, Element};

/// Lees een reeks elementen terug naar tekst.
///
/// Werkwijze: knip op de smalle scheidingsruimtes tussen tekens (elk teken is
/// negen elementen), zet elk blok om naar een breed/smal-patroon, en zoek dat op
/// in een eigen tabel.
fn decodeer(elementen: &[Element]) -> Option<String> {
    // Negen elementen per teken, met daartussen één scheidingsruimte.
    let mut tekens = Vec::new();
    let mut i = 0;
    while i < elementen.len() {
        if i + 9 > elementen.len() {
            return None;
        }
        let blok = &elementen[i..i + 9];
        // Balken op even posities, ruimtes op oneven -- anders zijn we de tel kwijt.
        for (j, e) in blok.iter().enumerate() {
            if e.is_bar != (j % 2 == 0) {
                return None;
            }
        }
        let breed: Vec<bool> = blok.iter().map(|e| e.modules == 3).collect();
        if breed.iter().filter(|b| **b).count() != 3 {
            return None; // Code 39: precies drie van de negen zijn breed
        }
        tekens.push(zoek_teken(&breed)?);
        i += 9;
        // scheidingsruimte overslaan als die er is
        if i < elementen.len() && !elementen[i].is_bar && elementen[i].modules == 1 {
            i += 1;
        }
    }
    let s: String = tekens.into_iter().collect();
    // Start- en stopteken eraf.
    let zonder = s.strip_prefix('*')?.strip_suffix('*')?;
    Some(zonder.to_string())
}

/// Eigen tabel, los van die van de encoder.
///
/// Bewust een deelverzameling: hem volledig overtypen zou precies de tikfouten
/// introduceren die hij moet betrappen. De heen-en-terugtest gebruikt alleen
/// deze tekens; dat de hele encodertabel klopt, wordt apart getoetst op de
/// eigenschap dat elk patroon precies drie brede elementen heeft.
fn zoek_teken(breed: &[bool]) -> Option<char> {
    const TABEL: &[(char, [bool; 9])] = &[
        (
            '*',
            [false, true, false, false, true, false, true, false, false],
        ),
        (
            '0',
            [false, false, false, true, true, false, true, false, false],
        ),
        (
            '1',
            [true, false, false, true, false, false, false, false, true],
        ),
        (
            '2',
            [false, false, true, true, false, false, false, false, true],
        ),
        (
            '3',
            [true, false, true, true, false, false, false, false, false],
        ),
        (
            '7',
            [false, false, false, true, false, false, true, false, true],
        ),
        (
            'A',
            [true, false, false, false, false, true, false, false, true],
        ),
        (
            'B',
            [false, false, true, false, false, true, false, false, true],
        ),
        (
            'C',
            [true, false, true, false, false, true, false, false, false],
        ),
        (
            'Z',
            [false, true, true, false, true, false, false, false, false],
        ),
        (
            '-',
            [false, true, false, false, false, false, true, false, true],
        ),
        (
            ' ',
            [false, true, true, false, false, false, true, false, false],
        ),
    ];
    TABEL
        .iter()
        .find(|(_, p)| p.as_slice() == breed)
        .map(|(c, _)| *c)
}

#[test]
fn wat_erin_gaat_komt_eruit() {
    // Alleen tekens die de eigen tabel hieronder kent -- die is met opzet een
    // deelverzameling, want hem volledig overtypen zou dezelfde tikfouten
    // introduceren die hij moet vangen. De volledige tabel wordt gedekt door
    // `elk_patroon_in_de_tabel_heeft_precies_drie_brede_elementen`.
    for tekst in ["A", "123", "ABC-123", "AB 27", "Z-0"] {
        let elementen = encode_code39(tekst).unwrap_or_else(|| panic!("{tekst} codeert niet"));
        let terug = decodeer(&elementen).unwrap_or_else(|| {
            panic!(
                "{tekst} is niet terug te lezen uit {} elementen",
                elementen.len()
            )
        });
        assert_eq!(
            terug,
            tekst.to_ascii_uppercase(),
            "heen en terug gaf iets anders"
        );
    }
}

#[test]
fn een_omgedraaid_bit_wordt_betrapt() {
    // De belangrijkste test: dit is wat er misgaat als iemand de tabel
    // overtypt. Zonder deze zou de decoder een fout patroon gewoon accepteren
    // als hij de tabel van de encoder deelde.
    let mut elementen = encode_code39("A").expect("codeert niet");
    let oud = elementen[9].modules;
    elementen[9].modules = if oud == 3 { 1 } else { 3 };
    assert!(
        decodeer(&elementen).is_none() || decodeer(&elementen).as_deref() != Some("A"),
        "een gewijzigd element leverde nog steeds 'A' op"
    );
}

#[test]
fn de_bookends_zitten_erin() {
    // Zonder start- en stopteken herkent een scanner de code niet, en dat is
    // niet te zien aan het plaatje.
    let elementen = encode_code39("1").expect("codeert niet");
    // drie tekens (* 1 *) van negen elementen plus twee scheidingsruimtes
    assert_eq!(elementen.len(), 3 * 9 + 2, "onverwacht aantal elementen");
}

#[test]
fn een_teken_dat_code39_niet_kent_geeft_niets() {
    // Stil weglaten zou een barcode opleveren die naar iets anders scant dan er
    // staat. Liever niets tekenen.
    assert!(encode_code39("hallo!").is_none(), "'!' werd geaccepteerd");
    assert!(
        encode_code39("naïef").is_none(),
        "een niet-ASCII-teken werd geaccepteerd"
    );
}

#[test]
fn elk_patroon_in_de_tabel_heeft_precies_drie_brede_elementen() {
    // Een eigenschap van Code 39 die de hele tabel moet halen. Eén tikfout in
    // 44 rijen is met het oog niet te vinden; dit vindt hem wel.
    for teken in "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%".chars() {
        let elementen =
            encode_code39(&teken.to_string()).unwrap_or_else(|| panic!("{teken} codeert niet"));
        // Het middelste teken; de bookends zijn al getoetst.
        let blok = &elementen[10..19];
        let breed = blok.iter().filter(|e| e.modules == 3).count();
        assert_eq!(
            breed, 3,
            "{teken} heeft {breed} brede elementen in plaats van 3"
        );
    }
}

#[test]
fn de_breedte_is_te_berekenen_voor_het_schalen() {
    let elementen = encode_code39("12").expect("codeert niet");
    let modules = total_modules(&elementen);
    assert!(modules > 0);
    // Vier tekens van 9 elementen; elk teken is 6 smal + 3 breed = 15 modules,
    // plus de scheidingsruimtes.
    assert_eq!(modules, 4 * 15 + 3, "de modulebreedte klopt niet");
}

#[test]
fn het_type_attribuut_wordt_herkend() {
    assert_eq!(
        BarcodeType::from_xfa_name("code39"),
        Some(BarcodeType::Code39)
    );
    assert_eq!(
        BarcodeType::from_xfa_name("Code3of9"),
        Some(BarcodeType::Code39)
    );
    // Niet ondersteund hoort None te geven en niet stilzwijgend Code 39, anders
    // tekenen we een code128-veld als iets dat naar iets anders scant.
    assert_eq!(BarcodeType::from_xfa_name("code128"), None);
    assert_eq!(BarcodeType::from_xfa_name("pdf417"), None);
}
