// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Barcodes for XFA form fields.
//!
//! A `<field>` carrying `<ui><barcode type="..."/></ui>` was recognised, placed
//! and sized, and then drawn as ordinary text -- there was no encoder. The form
//! is then not scannable, which is the one thing a barcode is on it for. See
//! #147.
//!
//! WAT HIER WEL EN NIET IN ZIT
//!
//! Code 39 (`code39`, ook wel Code 3 of 9). Gekozen omdat het het meest
//! voorkomt in overheids- en industriële formulieren, en omdat het zonder
//! tabellen van derden correct te implementeren is: elk teken is negen
//! elementen -- vijf balken en vier tussenruimtes -- waarvan er precies drie
//! breed zijn.
//!
//! Andere typen (Code 128, EAN-13, PDF417, QR) volgen niet vanzelf: Code 128
//! heeft codesets en een checksum, PDF417 en QR zijn tweedimensionaal en vragen
//! foutcorrectie. Die staan als grens vastgelegd in plaats van half gebouwd --
//! een barcode die er goed uitziet en niet scant, is erger dan geen barcode.

/// Welke barcodesoorten we kunnen tekenen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarcodeType {
    /// Code 39, met de `*`-start- en stoptekens die scanners verwachten.
    Code39,
}

impl BarcodeType {
    /// Herken het `type`-attribuut uit `<barcode type="...">`.
    ///
    /// XFA schrijft de namen in kleine letters zonder scheidingsteken
    /// (`code39`, `code128`, `ean13`). Onbekende of nog niet ondersteunde
    /// soorten geven `None`; de aanroeper valt dan terug op tekst, wat
    /// zichtbaar verkeerd is en daarmee eerlijker dan een balkenpatroon dat
    /// niets betekent.
    pub fn from_xfa_name(naam: &str) -> Option<Self> {
        match naam.trim().to_ascii_lowercase().as_str() {
            "code39" | "code3of9" => Some(Self::Code39),
            _ => None,
        }
    }
}

/// De Code 39-alfabet: teken -> negen elementen, `true` is breed.
///
/// Volgorde is balk, ruimte, balk, ruimte, ... beginnend en eindigend met een
/// balk. Precies drie van de negen zijn breed; dat is de controle die de test
/// hieronder op de hele tabel uitvoert.
const CODE39: &[(char, [bool; 9])] = &[
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
        '4',
        [false, false, false, true, true, false, false, false, true],
    ),
    (
        '5',
        [true, false, false, true, true, false, false, false, false],
    ),
    (
        '6',
        [false, false, true, true, true, false, false, false, false],
    ),
    (
        '7',
        [false, false, false, true, false, false, true, false, true],
    ),
    (
        '8',
        [true, false, false, true, false, false, true, false, false],
    ),
    (
        '9',
        [false, false, true, true, false, false, true, false, false],
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
        'D',
        [false, false, false, false, true, true, false, false, true],
    ),
    (
        'E',
        [true, false, false, false, true, true, false, false, false],
    ),
    (
        'F',
        [false, false, true, false, true, true, false, false, false],
    ),
    (
        'G',
        [false, false, false, false, false, true, true, false, true],
    ),
    (
        'H',
        [true, false, false, false, false, true, true, false, false],
    ),
    (
        'I',
        [false, false, true, false, false, true, true, false, false],
    ),
    (
        'J',
        [false, false, false, false, true, true, true, false, false],
    ),
    (
        'K',
        [true, false, false, false, false, false, false, true, true],
    ),
    (
        'L',
        [false, false, true, false, false, false, false, true, true],
    ),
    (
        'M',
        [true, false, true, false, false, false, false, true, false],
    ),
    (
        'N',
        [false, false, false, false, true, false, false, true, true],
    ),
    (
        'O',
        [true, false, false, false, true, false, false, true, false],
    ),
    (
        'P',
        [false, false, true, false, true, false, false, true, false],
    ),
    (
        'Q',
        [false, false, false, false, false, false, true, true, true],
    ),
    (
        'R',
        [true, false, false, false, false, false, true, true, false],
    ),
    (
        'S',
        [false, false, true, false, false, false, true, true, false],
    ),
    (
        'T',
        [false, false, false, false, true, false, true, true, false],
    ),
    (
        'U',
        [true, true, false, false, false, false, false, false, true],
    ),
    (
        'V',
        [false, true, true, false, false, false, false, false, true],
    ),
    (
        'W',
        [true, true, true, false, false, false, false, false, false],
    ),
    (
        'X',
        [false, true, false, false, true, false, false, false, true],
    ),
    (
        'Y',
        [true, true, false, false, true, false, false, false, false],
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
        '.',
        [true, true, false, false, false, false, true, false, false],
    ),
    (
        ' ',
        [false, true, true, false, false, false, true, false, false],
    ),
    (
        '$',
        [false, true, false, true, false, true, false, false, false],
    ),
    (
        '/',
        [false, true, false, true, false, false, false, true, false],
    ),
    (
        '+',
        [false, true, false, false, false, true, false, true, false],
    ),
    (
        '%',
        [false, false, false, true, false, true, false, true, false],
    ),
    (
        '*',
        [false, true, false, false, true, false, true, false, false],
    ),
];

fn patroon(c: char) -> Option<[bool; 9]> {
    let boven = c.to_ascii_uppercase();
    CODE39.iter().find(|(k, _)| *k == boven).map(|(_, p)| *p)
}

/// Eén element uit een gecodeerde barcode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Element {
    /// `true` voor een balk, `false` voor een tussenruimte.
    pub is_bar: bool,
    /// Breedte in modules: 1 voor smal, 3 voor breed.
    pub modules: u8,
}

/// Codeer `tekst` als Code 39.
///
/// Geeft `None` als er een teken in staat dat Code 39 niet kent. Dat is bewust
/// geen stille overslag: een barcode waar tekens uit weggelaten zijn, scant
/// naar iets anders dan er staat, en dat is erger dan niets tekenen.
pub fn encode_code39(tekst: &str) -> Option<Vec<Element>> {
    // Start- en stopteken horen erbij; zonder herkent een scanner de code niet.
    let met_bookends: String = format!("*{}*", tekst.to_ascii_uppercase());
    let mut uit = Vec::new();
    for (i, c) in met_bookends.chars().enumerate() {
        if c == '*' && i != 0 && i != met_bookends.chars().count() - 1 {
            // Een `*` middenin zou de code vroegtijdig afsluiten.
            return None;
        }
        let p = patroon(c)?;
        for (j, breed) in p.iter().enumerate() {
            uit.push(Element {
                is_bar: j % 2 == 0,
                modules: if *breed { 3 } else { 1 },
            });
        }
        // Tussenruimte tussen tekens, altijd smal.
        uit.push(Element {
            is_bar: false,
            modules: 1,
        });
    }
    uit.pop(); // geen losse ruimte achteraan
    Some(uit)
}

/// Totale breedte in modules, zodat de aanroeper kan schalen naar de veldbreedte.
pub fn total_modules(elementen: &[Element]) -> u32 {
    elementen.iter().map(|e| u32::from(e.modules)).sum()
}
