// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

    let report = doc.validate_pdfa(PdfAProfile::A2b)?;
    if report.is_compliant() {
        println!("conforms to PDF/A-2b");
    } else {
        for violation in &report.violations {
            println!("{violation:?}");
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
        smoke_page_count();
        let _ = open_and_count(pad);
        quickstart_extract_text();
        let _ = text_and_merge(pad);
        let _ = xfa_fill_and_flatten(pad);
        let _ = pdfa_validate_and_convert(pad);
        let _ = sign_and_verify(pad);
        let _ = render_pages_to_png(pad);
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

/// De installatiepagina's "does it work" -- een heel programma, niet een fragment.
///
/// Dit blok en de twee hieronder stonden als sjabloonliteral in TSX
/// (`docsChannels.ts`, `DownloadPage.tsx`, `SdkPage.tsx`). Ze klopten toevallig,
/// maar niets hield dat vast: acht Rust-blokken op de site droegen hun eigen
/// tekst en geen enkele was ooit gebouwd (#247).
///
/// De geneste `fn main` is met opzet: wat de lezer op de installatiepagina ziet
/// is een compleet programma om te plakken, en dat is dus ook wat hier
/// compileert.
fn smoke_page_count() {
    // site:smoke-page-count
    use pdfluent::PdfDocument;

    fn main() -> pdfluent::Result<()> {
        let doc = PdfDocument::open("invoice.pdf")?;
        println!("{} pages", doc.page_count());
        Ok(())
    }
    // site:end
    let _ = main();
}

/// Hetzelfde, zonder programma-omhulsel: de downloadpagina toont het fragment.
fn open_and_count(path: &Path) -> pdfluent::Result<()> {
    // site:open-and-count
    use pdfluent::PdfDocument;

    let doc = PdfDocument::open("invoice.pdf")?;
    println!("{} pages", doc.page_count());
    // site:end
    let _ = (path, doc);
    Ok(())
}

/// De SDK-pagina: openen en de tekst eruit, als heel programma.
fn quickstart_extract_text() {
    // site:quickstart-extract-text
    use pdfluent::PdfDocument;

    fn main() -> pdfluent::Result<()> {
        let doc = PdfDocument::open("invoice.pdf")?;
        let text = doc.extract_text()?;
        println!("{text}");
        Ok(())
    }
    // site:end
    let _ = main();
}

/// Tekst met plaatsaanduiding, en documenten samenvoegen.
fn text_and_merge(path: &Path) -> pdfluent::Result<()> {
    // site:text-and-merge
    use pdfluent::{PdfDocument, PdfMerger};

    let doc = PdfDocument::open("report.pdf")?;

    // Text with its bounding box, block by block.
    for block in doc.text_with_layout()? {
        let [x, y, _, _] = block.bbox;
        println!("p{} [{x:.0},{y:.0}] {}", block.page, block.text);
    }

    // Merge multiple PDFs.
    let merged = PdfMerger::new()
        .add(PdfDocument::open("part1.pdf")?)
        .add(PdfDocument::open("part2.pdf")?)
        .build()?;
    merged.save("combined.pdf")?;
    // site:end
    let _ = path;
    Ok(())
}

/// Een XFA-formulier lezen en invullen.
///
/// De site schreef hier `doc.flatten_xfa()?`. Die methode bestaat niet, en een
/// naamcontrole ziet dat niet -- `flatten` bestaat immers. De lezer kreeg E0599
/// op de laatste stap van de belangrijkste demonstratie op die pagina.
///
/// Wat er niet voor in de plaats komt is `flatten_forms`. Dat compileert, en
/// dat is precies de val: het is AcroForm-only, en in 1.0 geeft het altijd
/// `Error::MissingDependency` terug (#1223). Een voorbeeld dat bouwt en bij
/// elke uitvoering faalt is erger dan een voorbeeld dat niet bouwt -- dezelfde
/// vorm als `SignOptions::new()` hierboven. Het blok stopt dus na het invullen,
/// en toont wat er wél gebeurt: de waarde gaat de datasets-packet in.
fn xfa_fill_and_flatten(path: &Path) -> pdfluent::Result<()> {
    // site:xfa-fill-and-flatten
    use pdfluent::{PdfDocument, XfaFieldValue};

    let mut doc = PdfDocument::open("tax_return.pdf")?;

    // The form model: every logical field, with its value and its type.
    let model = doc.xfa_form_model()?;
    for f in &model.fields {
        println!("{} = {:?} ({:?})", f.name, f.value, f.field_type);
    }

    // Fill by name. The value is written back into the datasets packet, so
    // saving keeps it.
    let value = XfaFieldValue::Text("Alice Smith");
    let outcome = doc.set_xfa_field_value("form1.applicant.name", value)?;
    println!("persisted: {}", outcome.persisted_to_datasets);
    doc.save("filed_return.pdf")?;
    // site:end
    let _ = path;
    Ok(())
}

/// PDF/A toetsen en daarna omzetten.
fn pdfa_validate_and_convert(path: &Path) -> pdfluent::Result<()> {
    // site:pdfa-validate-and-convert
    use pdfluent::{PdfAProfile, PdfDocument};

    let doc = PdfDocument::open("legacy.pdf")?;

    // Validate, then read the findings.
    let report = doc.validate_pdfa(PdfAProfile::A2b)?;
    if !report.is_compliant() {
        for v in &report.violations {
            eprintln!("[{}] {} ({:?})", v.rule, v.message, v.severity);
        }
    }

    // convert_to_pdfa returns a new document rather than changing this one.
    let archived = doc.convert_to_pdfa(PdfAProfile::A2b)?;
    archived.save("archived.pdf")?;
    // site:end
    let _ = path;
    Ok(())
}

/// Ondertekenen en daarna verifiëren, in één blok.
///
/// De site liet `SignOptions::new()` staan. Dat vraagt PAdES B-LT, en deze build
/// weigert dat: er is geen route om een document security store te schrijven
/// (#176). Zo'n voorbeeld compileert en faalt pas bij de eerste uitvoering --
/// het slechtste van beide. Het profiel staat er nu expliciet in.
fn sign_and_verify(path: &Path) -> pdfluent::Result<()> {
    // site:sign-and-verify
    use pdfluent::{PadesProfile, PdfDocument, Pkcs12Signer, SignOptions};

    let mut doc = PdfDocument::open("contract.pdf")?;

    let signer = Pkcs12Signer::from_pfx_file("cert.p12", "password")?;
    doc.sign(
        &signer,
        SignOptions::new()
            // The default profile is PAdES B-LT, which this build refuses:
            // it has no route to write a document security store.
            .profile(PadesProfile::BasicSignature)
            .reason("Approved by legal")
            .location("Amsterdam, NL"),
    )?;
    doc.save("signed_contract.pdf")?;

    // Verify the signatures already on a document.
    let report = doc.verify_signatures()?;
    for v in report.validations() {
        println!("{}: {:?}", v.info.signer_name, v.status);
    }
    // site:end
    let _ = path;
    Ok(())
}

/// Elke bladzijde als PNG.
///
/// De benchmarkpagina toonde `use pdfluent::{Document, RenderOptions};` -- twee
/// namen die geen van beide bestaan, op de pagina die rendersnelheid claimt.
/// Rasteren gaat via `to_images`, dat de bestanden zelf wegschrijft en de paden
/// teruggeeft. De vorm klopte ook niet: `ToImagesOptions` is `#[non_exhaustive]`
/// en neemt geen structliteraal aan, dus zelfs met de juiste naam was
/// `RenderOptions { dpi: 150, ..Default::default() }` niet te bouwen (E0639).
fn render_pages_to_png(path: &Path) -> pdfluent::Result<()> {
    // site:render-pages-to-png
    use pdfluent::{ImageFormat, PdfDocument, ToImagesOptions};

    let doc = PdfDocument::open("document.pdf")?;

    // The page number goes in as `{page}`. Without that marker `to_images`
    // appends `_N` before the extension instead, so a pattern is never wrong
    // in a way the compiler can see.
    //
    // ToImagesOptions is #[non_exhaustive]: it takes the builder, not a struct
    // literal, so that a new option is not a breaking change.
    let report = doc.to_images(
        "page_{page}.png",
        ToImagesOptions::new()
            .with_dpi(150)
            .with_format(ImageFormat::Png),
    )?;
    println!("{} pages rendered", report.paths.len());
    // site:end
    let _ = path;
    Ok(())
}
