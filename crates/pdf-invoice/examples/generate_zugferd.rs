//! Generate ZUGFeRD / Factur-X test PDFs for all five profiles.
//!
//! Usage:
//!   cargo run -p pdf-invoice --example generate_zugferd [outdir]
//!
//! Writes five PDF files to `outdir` (default: /tmp/zugferd-generated/):
//!   zugferd-minimum.pdf, zugferd-basicwl.pdf, zugferd-basic.pdf,
//!   zugferd-en16931.pdf, zugferd-extended.pdf

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use chrono::NaiveDate;
use lopdf::{Dictionary, Document, Object};
use pdf_invoice::embed::{embed_xml_attachment, AfRelationship};
use pdf_invoice::zugferd::{
    Address, LineItem, PaymentMeans, TaxCategory, TradeParty, ZugferdInvoice, ZugferdProfile,
};

fn main() {
    let outdir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/zugferd-generated".to_string());
    std::fs::create_dir_all(&outdir).expect("create outdir");

    let profiles = [
        (
            ZugferdProfile::Minimum,
            "zugferd-minimum.pdf",
            "factur-x.xml",
        ),
        (
            ZugferdProfile::BasicWL,
            "zugferd-basicwl.pdf",
            "factur-x.xml",
        ),
        (ZugferdProfile::Basic, "zugferd-basic.pdf", "factur-x.xml"),
        (
            ZugferdProfile::EN16931,
            "zugferd-en16931.pdf",
            "factur-x.xml",
        ),
        (
            ZugferdProfile::Extended,
            "zugferd-extended.pdf",
            "factur-x.xml",
        ),
    ];

    for (profile, filename, xml_name) in &profiles {
        let path = format!("{}/{}", outdir, filename);
        match generate_pdf(*profile, xml_name) {
            Ok(bytes) => {
                std::fs::write(&path, &bytes).expect("write PDF");
                println!("Written: {path}");
            }
            Err(e) => eprintln!("Error generating {filename}: {e}"),
        }
    }
}

fn make_minimal_pdf() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.add_object(Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Pages".to_vec())),
        ("Kids", Object::Array(vec![])),
        ("Count", Object::Integer(0)),
    ]));
    let catalog_id = doc.add_object(Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Catalog".to_vec())),
        ("Pages", Object::Reference(pages_id)),
    ]));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

fn make_invoice(profile: ZugferdProfile) -> ZugferdInvoice {
    let needs_line_items = profile.requires_line_items();
    let needs_tax_id = matches!(profile, ZugferdProfile::EN16931 | ZugferdProfile::Extended);
    let issue_date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();

    let line_items = if needs_line_items {
        vec![
            LineItem {
                id: "1".into(),
                description: "Software license".into(),
                quantity: 1.0,
                unit_code: "C62".into(),
                unit_price: 500.0,
                price_base_quantity: 1.0,
                line_total: 500.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            },
            LineItem {
                id: "2".into(),
                description: "Support (12 months)".into(),
                quantity: 12.0,
                unit_code: "MON".into(),
                unit_price: 50.0,
                price_base_quantity: 1.0,
                line_total: 600.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            },
        ]
    } else {
        vec![]
    };

    ZugferdInvoice {
        profile,
        invoice_number: format!("XFA-2026-{profile:?}"),
        type_code: "380".into(),
        issue_date,
        seller: TradeParty {
            name: "XFA Solutions B.V.".into(),
            address: Address {
                street: Some("Keizersgracht 100".into()),
                city: Some("Amsterdam".into()),
                postal_code: Some("1015 AA".into()),
                country_code: "NL".into(),
            },
            tax_id: if needs_tax_id {
                Some("NL123456789B01".into())
            } else {
                None
            },
            registration_id: Some("12345678".into()),
            email: Some("billing@xfa.nl".into()),
        },
        buyer: TradeParty {
            name: "Acme GmbH".into(),
            address: Address {
                street: Some("Hauptstr. 42".into()),
                city: Some("Berlin".into()),
                postal_code: Some("10115".into()),
                country_code: "DE".into(),
            },
            tax_id: None,
            registration_id: None,
            email: None,
        },
        line_items,
        currency: "EUR".into(),
        tax_basis_total: 1100.0,
        tax_total: 231.0,
        grand_total: 1331.0,
        due_payable: 1331.0,
        charge_total: 0.0,
        allowance_total: 0.0,
        payment_means: Some(PaymentMeans {
            type_code: "58".into(),
            information: Some("SEPA credit transfer".into()),
        }),
        payment_terms: Some(pdf_invoice::zugferd::PaymentTerms {
            description: Some("Net 30 days".into()),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()),
        }),
        buyer_reference: Some("PO-2026-001".into()),
    }
}

fn generate_pdf(
    profile: ZugferdProfile,
    xml_filename: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let invoice = make_invoice(profile);
    let xml = invoice.to_xml()?;
    let mut doc = make_minimal_pdf();
    embed_xml_attachment(&mut doc, xml_filename, xml.as_bytes(), AfRelationship::Data)?;
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}
