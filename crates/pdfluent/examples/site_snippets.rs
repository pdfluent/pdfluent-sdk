// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! De codevoorbeelden van pdfluent.com, als code die bouwt.
//!
//! De documentatiepagina droeg vier Rust-voorbeelden die **niet compileerden**.
//! Ze verwezen naar een `pdfluent::Sdk` die niet bestaat, naar
//! `Sdk::init_with_license("license.json")`, naar `doc.import_xfa_data(...)`
//! dat nergens voorkomt, en naar `PdfaLevel` waar het type `PdfAProfile` heet.
//! Wie het eerste voorbeeld kopieerde kreeg vijf compilatiefouten op vier
//! regels code -- als eerste indruk van de SDK.
//!
//! Niemand had ze ooit gebouwd. Was dat gebeurd, dan was het er meteen
//! uitgekomen. Zie #164.
//!
//! Dit bestand is de bron. Elk blok tussen `// site:<naam>` en `// site:end`
//! is een voorbeeld dat op de site hoort, en dit bestand compileert in CI --
//! dus een voorbeeld dat niet meer klopt, breekt de build in plaats van de
//! eerste indruk van een bezoeker.
//!
//! De voorbeelden gebruiken geen licentiesleutel. Dat is geen verzuim maar het
//! besluit van 24-08-2026: de SDK staat open, alleen ondertekenen weigert
//! zonder licentie.

use std::path::Path;

// Elk blok draagt zijn eigen `use`-regels, binnen de markering: een fragment
// zonder imports is niet te plakken, en dat was juist de klacht (#164, #247).

/// Tekst uit een document halen.
fn open_and_extract(path: &Path) -> pdfluent::Result<()> {
    // site:open-and-extract
    use pdfluent::PdfDocument;

    let doc = PdfDocument::open("invoice.pdf")?;

    // The full text, in reading order.
    let text = doc.extract_text()?;
    println!("{text}");

    // Or per page.
    for n in 0..doc.page_count() {
        println!("--- page {} ---", n + 1);
        println!("{}", doc.page(n)?.text()?);
    }
    // site:end
    let _ = (path, doc);
    Ok(())
}

/// Een XFA-formulier invullen en afvlakken.
fn fill_xfa(path: &Path) -> pdfluent::Result<()> {
    // site:fill-xfa
    use pdfluent::PdfDocument;

    let mut doc = PdfDocument::open("tax_return.pdf")?;

    // The form model: one row per logical field.
    let model = doc.xfa_form_model()?;
    for field in &model.fields {
        println!("{} = {:?}", field.name, field.value);
    }

    // Fill by name.
    doc.set_xfa_field_value("form1.name", pdfluent::xfa::XfaFieldValue::Text("Smith"))?;
    // site:end
    let _ = path;
    Ok(())
}

/// Toetsen of een document aan PDF/A voldoet.
fn validate_pdfa(path: &Path) -> pdfluent::Result<()> {
    // site:validate-pdfa
    use pdfluent::{PdfAProfile, PdfDocument};

    let doc = PdfDocument::open("legacy.pdf")?;

    let rapport = doc.validate_pdfa(PdfAProfile::A2b)?;
    if rapport.is_compliant() {
        println!("conform PDF/A-2b");
    } else {
        for schending in &rapport.violations {
            println!("{schending:?}");
        }
    }
    // site:end
    let _ = path;
    Ok(())
}

fn main() {
    // Dit bestand bestaat om te compileren, niet om te draaien: de genoemde
    // bestanden staan er niet. Wat het bewijst is dat de voorbeelden op de
    // site kloppen met de API die er werkelijk is.
    let pad = Path::new("voorbeeld.pdf");
    if std::env::var("PDFLUENT_SITE_SNIPPETS_RUN").is_ok() {
        let _ = open_and_extract(pad);
        let _ = fill_xfa(pad);
        let _ = validate_pdfa(pad);
        let _ = sign_pkcs12(pad);
        let _ = verify_signatures(pad);
        let _ = after_chrome(pad);
    } else {
        println!(
            "site_snippets: gebouwd, niet gedraaid (zet PDFLUENT_SITE_SNIPPETS_RUN om te draaien)"
        );
    }
}

/// Een document ondertekenen met een PKCS#12-bestand.
///
/// De site beschreef `PdfSigner::from_pkcs12("cert.p12", "password")` en deed
/// vervolgens `let signed = doc.sign(...)` gevolgd door `signed.save(...)`.
/// Drie dingen kloppen daar niet: de constructor heet `Pkcs12Signer::from_pfx_file`,
/// `sign` wijzigt het document en levert niets op, en het profiel dat
/// `SignOptions::new()` kiest is `LongTerm` -- dat weigert deze build, omdat er
/// geen route is om een document security store te schrijven (#176).
///
/// Een voorbeeld dat het standaardprofiel laat staan, faalt dus bij de eerste
/// uitvoering. Het kiest hier expliciet `BasicSignature`.
fn sign_pkcs12(path: &Path) -> pdfluent::Result<()> {
    // site:sign-pkcs12
    use pdfluent::signer::{PadesProfile, Pkcs12Signer, SignOptions};
    use pdfluent::PdfDocument;

    let mut doc = PdfDocument::open("contract.pdf")?;
    let signer = Pkcs12Signer::from_pfx_file("cert.p12", "password")?;

    // SignOptions::new() asks for PAdES B-LT, which needs a document security
    // store this build cannot write. Choose the profile you can honour.
    let opts = SignOptions::new()
        .profile(PadesProfile::BasicSignature)
        .reason("Approved");

    doc.sign(&signer, opts)?;
    doc.save("contract-signed.pdf")?;
    // site:end
    let _ = path;
    Ok(())
}

/// Handtekeningen verifiëren.
///
/// De site beschreef hier een `TrustStore` en `SignatureVerifier::with_trust_store()`.
/// Geen van beide bestaat. Erger dan de verkeerde naam is wat het beloofde: dat je
/// je eigen CA kon vastpinnen. Dat kan niet — de keten wordt gebouwd uit de
/// certificaten die in het document zitten, zonder vertrouwensanker.
fn verify_signatures(path: &Path) -> pdfluent::Result<()> {
    // site:verify-signatures
    use pdfluent::signer::SignatureStatus;
    use pdfluent::PdfDocument;

    let doc = PdfDocument::open("signed.pdf")?;
    let report = doc.verify_signatures()?;
    for validation in report.validations() {
        match &validation.status {
            SignatureStatus::Valid => println!("Signature is valid"),
            SignatureStatus::Invalid { reason } => println!("Signature is invalid: {reason}"),
            SignatureStatus::Unknown { reason } => {
                println!("Signature status is unknown: {reason}")
            }
            // SignatureStatus is #[non_exhaustive]: new outcomes can be added
            // without a breaking change, so a match on it needs a catch-all.
            _ => println!("Signature status not recognised by this build"),
        }
    }
    // site:end
    let _ = (path, doc);
    Ok(())
}

/// Wat PDFluent doet met een PDF die headless Chrome heeft gemaakt.
///
/// De site beschreef hier `pdfluent::HtmlToPdf` met een "browser bridge" —
/// een hele API die niet bestaat, voor iets dat we op 19-08-2026 hebben
/// besloten niet te bouwen. Wat wél klopt is de tweede helft: de conversie doe
/// je met Chrome, en alles daarna met PDFluent.
///
/// Dit blok bestaat omdat een taalmodel de vervangende code niet kon schrijven
/// zonder hem te verzinnen: het stelde `doc.compress().add_watermark("DRAFT")`
/// voor, en geen van die twee heeft die vorm. De compiler wist dat wel.
fn after_chrome(path: &Path) -> pdfluent::Result<()> {
    // site:after-chrome
    use pdfluent::{CompressOptions, PdfAProfile, PdfDocument, WatermarkOptions};

    // Chrome wrote the PDF; everything after that is PDFluent.
    let mut doc = PdfDocument::open("out.pdf")?;

    doc.add_watermark("DRAFT", WatermarkOptions::centered())?;
    let report = doc.compress(CompressOptions::default())?;
    println!("{} streams compressed", report.streams_compressed);

    // convert_to_pdfa returns a new document rather than changing this one.
    let archived = doc.convert_to_pdfa(PdfAProfile::A2b)?;
    archived.save("invoice-archived.pdf")?;
    // site:end
    let _ = path;
    Ok(())
}
