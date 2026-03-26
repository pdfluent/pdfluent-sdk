use chrono::NaiveDate;
use pdf_invoice::zugferd::{
    Address, LineItem, PaymentMeans, PaymentTerms, TaxCategory, TradeParty, ZugferdInvoice,
    ZugferdProfile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/xrechnung-sample.xml".to_string());

    let invoice = ZugferdInvoice {
        profile: ZugferdProfile::EN16931,
        invoice_number: "XRE-2026-001".into(),
        type_code: "380".into(),
        issue_date: NaiveDate::from_ymd_opt(2026, 3, 26).unwrap(),
        seller: TradeParty {
            name: "XFA Solutions B.V.".into(),
            address: Address {
                street: Some("Keizersgracht 100".into()),
                city: Some("Amsterdam".into()),
                postal_code: Some("1015 AA".into()),
                country_code: "NL".into(),
            },
            tax_id: Some("NL123456789B01".into()),
            registration_id: Some("12345678".into()),
            email: Some("billing@xfa.nl".into()),
        },
        buyer: TradeParty {
            name: "Bundesdruckerei GmbH".into(),
            address: Address {
                street: Some("Kommandantenstrasse 18".into()),
                city: Some("Berlin".into()),
                postal_code: Some("10969".into()),
                country_code: "DE".into(),
            },
            tax_id: Some("DE123456789".into()),
            registration_id: None,
            email: Some("eingang@bundesdruckerei.de".into()),
        },
        line_items: vec![
            LineItem {
                id: "1".into(),
                description: "PDF processing platform".into(),
                quantity: 2.0,
                unit_code: "C62".into(),
                unit_price: 750.0,
                price_base_quantity: 1.0,
                line_total: 1500.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            },
            LineItem {
                id: "2".into(),
                description: "Maintenance".into(),
                quantity: 12.0,
                unit_code: "MON".into(),
                unit_price: 50.0,
                price_base_quantity: 1.0,
                line_total: 600.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            },
        ],
        currency: "EUR".into(),
        tax_basis_total: 2100.0,
        tax_total: 441.0,
        grand_total: 2541.0,
        due_payable: 2541.0,
        charge_total: 0.0,
        allowance_total: 0.0,
        payment_means: Some(PaymentMeans {
            type_code: "58".into(),
            information: Some("SEPA credit transfer".into()),
        }),
        payment_terms: Some(PaymentTerms {
            description: Some("Net 30 days".into()),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 4, 25).unwrap()),
        }),
        buyer_reference: Some("04011000-12345-03".into()),
    };

    let xml = invoice.to_xml()?;
    std::fs::write(&output_path, xml)?;
    println!("Written XRechnung XML to {}", output_path);

    Ok(())
}
