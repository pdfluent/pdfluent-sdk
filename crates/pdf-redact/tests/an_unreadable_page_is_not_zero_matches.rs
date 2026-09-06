// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Redactie mag niet "nul treffers" melden op een pagina die het niet kon lezen.
//!
//! `search_and_redact` sloeg zo'n pagina over met `Err(_) => continue`. De
//! aanroeper kreeg dan een schoon rapport terug én een document waar niets uit
//! was gehaald. Voor een AVG-functie is dat de ergste vorm die een fout kan
//! aannemen, want hij ziet eruit als succes.
//!
//! Het onderscheid dat telt: een **lege** pagina is gelezen en had geen tekst
//! -- dat is een geldig antwoord. Een **onleesbare** pagina is geen antwoord.

use lopdf::{Document, Object};
use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};
use test_skip::skip_test;

fn corpus(naam: &str) -> Option<Vec<u8>> {
    let pad = format!("../../tests/corpus-mini/{naam}");
    match std::fs::read(&pad) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("SKIPPED (not a pass): {pad} ontbreekt");
            None
        }
    }
}

/// Op een leesbaar document blijft de lijst leeg.
#[test]
fn een_leesbaar_document_meldt_geen_onleesbare_paginas() {
    let Some(bytes) = corpus("simple.pdf") else {
        skip_test!("corpus fixture simple.pdf is not there")
    };
    let mut doc = Document::load_mem(&bytes).expect("simple.pdf laadt");
    let rapport = search_and_redact(&mut doc, "Hello", &RedactSearchOptions::default())
        .expect("redactie draait");
    assert!(
        rapport.pages_unreadable.is_empty(),
        "een leesbaar document meldt onleesbare pagina's: {:?}",
        rapport.pages_unreadable
    );
}

/// Een pagina waarvan de inhoudsstroom niet bestaat, wordt gemeld.
///
/// De verwijzing wordt met opzet naar een objectnummer gezet dat er niet is --
/// precies wat er gebeurde toen acht fixtures `<<//Length` droegen en het
/// object daardoor niet laadde.
#[test]
fn een_pagina_zonder_leesbare_inhoud_wordt_gemeld() {
    let Some(bytes) = corpus("simple.pdf") else {
        skip_test!("corpus fixture simple.pdf is not there")
    };
    let mut doc = Document::load_mem(&bytes).expect("simple.pdf laadt");

    let paginas: Vec<_> = doc.get_pages().into_iter().collect();
    let (paginanummer, page_id) = paginas.first().copied().expect("minstens een pagina");
    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        dict.set("Contents", Object::Reference((9999, 0)));
    }

    let rapport = search_and_redact(&mut doc, "Hello", &RedactSearchOptions::default())
        .expect("redactie draait ook op een kapotte pagina");

    assert_eq!(
        rapport.matches_found, 0,
        "er kan niets gevonden zijn op een pagina die niet gelezen kon worden"
    );
    assert!(
        rapport.pages_unreadable.contains(&paginanummer),
        "pagina {paginanummer} was onleesbaar maar staat niet in het rapport: {:?}. \
         Zonder die melding is 'niets gevonden' niet te onderscheiden van \
         'niet gekeken'.",
        rapport.pages_unreadable
    );
}

/// Een lege pagina is iets anders dan een onleesbare.
///
/// Zonder dit onderscheid zou de melding bij elk document met een blanco
/// pagina afgaan, en een waarschuwing die altijd afgaat leest niemand meer.
#[test]
fn een_lege_pagina_is_geen_onleesbare_pagina() {
    let Some(bytes) = corpus("acroform-multiselect.pdf") else {
        skip_test!("corpus fixture acroform-multiselect.pdf is not there")
    };
    let mut doc = Document::load_mem(&bytes).expect("acroform-multiselect.pdf laadt");
    let rapport = search_and_redact(
        &mut doc,
        "zzzq-bestaat-niet",
        &RedactSearchOptions::default(),
    )
    .expect("redactie draait");
    assert!(
        rapport.pages_unreadable.is_empty(),
        "een lege pagina wordt als onleesbaar gemeld: {:?}",
        rapport.pages_unreadable
    );
}

/// En een pagina die helemaal geen `/Contents` heeft, ook niet.
///
/// Dat is een andere tak dan hierboven: daar staat een verwijzing naar een
/// lege stroom, hier staat er niets. Allebei geldig, allebei geen melding
/// waard -- maar het zijn twee takken in de code en dus twee tests.
#[test]
fn een_pagina_zonder_contents_is_geen_onleesbare_pagina() {
    let Some(bytes) = corpus("simple.pdf") else {
        skip_test!("corpus fixture simple.pdf is not there")
    };
    let mut doc = Document::load_mem(&bytes).expect("simple.pdf laadt");

    let paginas: Vec<_> = doc.get_pages().into_iter().collect();
    let (_, page_id) = paginas.first().copied().expect("minstens een pagina");
    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        dict.remove(b"Contents");
    }

    let rapport = search_and_redact(&mut doc, "Hello", &RedactSearchOptions::default())
        .expect("redactie draait");
    assert!(
        rapport.pages_unreadable.is_empty(),
        "een pagina zonder /Contents is blanco, geen fout: {:?}",
        rapport.pages_unreadable
    );
}
