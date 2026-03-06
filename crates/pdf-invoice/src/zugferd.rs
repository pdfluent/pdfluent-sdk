//! ZUGFeRD 2.3 / Factur-X e-invoicing — CII XML generation and parsing.
//!
//! Implements the UN/CEFACT Cross-Industry Invoice (CII) D16B format used by
//! ZUGFeRD 2.3 and Factur-X 1.0.  The generated XML can be embedded into a
//! PDF/A-3 document (see [`crate::embed`]) to create a fully compliant
//! hybrid e-invoice.
//!
//! # Profiles
//!
//! ZUGFeRD defines five profiles with increasing detail:
//!
//! | Profile   | Description                              |
//! |-----------|------------------------------------------|
//! | Minimum   | Invoice reference only                   |
//! | BasicWL   | Structured data without line items       |
//! | Basic     | Structured data with line items          |
//! | EN16931   | EU standard (most commonly required)     |
//! | Extended  | Full CII feature set                     |

use crate::error::{InvoiceError, Result};
use chrono::NaiveDate;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::Cursor;

// -- CII XML namespaces --

const NS_RSM: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
const NS_RAM: &str =
    "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";
const NS_QDT: &str = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100";
const NS_UDT: &str = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100";

/// ZUGFeRD conformance profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ZugferdProfile {
    /// Minimum — invoice reference, date, totals only.
    Minimum,
    /// BasicWL — structured header data, no line items.
    BasicWL,
    /// Basic — header data plus line items.
    Basic,
    /// EN16931 — EU e-invoice standard (Directive 2014/55/EU).
    EN16931,
    /// Extended — full CII feature set.
    Extended,
}

impl ZugferdProfile {
    /// URN identifier used in the CII ExchangedDocumentContext.
    pub fn urn(self) -> &'static str {
        match self {
            Self::Minimum => "urn:factur-x.eu:1p0:minimum",
            Self::BasicWL => "urn:factur-x.eu:1p0:basicwl",
            Self::Basic => "urn:factur-x.eu:1p0:basic",
            Self::EN16931 => "urn:cen.eu:en16931:2017",
            Self::Extended => "urn:factur-x.eu:1p0:extended",
        }
    }

    /// Whether this profile requires line items.
    pub fn requires_line_items(self) -> bool {
        matches!(self, Self::Basic | Self::EN16931 | Self::Extended)
    }
}

/// A complete ZUGFeRD / Factur-X invoice.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ZugferdInvoice {
    /// Conformance profile.
    pub profile: ZugferdProfile,
    /// Invoice number.
    pub invoice_number: String,
    /// Type code (380 = commercial invoice, 381 = credit note).
    pub type_code: String,
    /// Issue date.
    pub issue_date: NaiveDate,
    /// Seller party.
    pub seller: TradeParty,
    /// Buyer party.
    pub buyer: TradeParty,
    /// Line items (required for Basic/EN16931/Extended).
    pub line_items: Vec<LineItem>,
    /// ISO 4217 currency code (e.g., "EUR").
    pub currency: String,
    /// Total amount excluding tax.
    pub tax_basis_total: f64,
    /// Total tax amount.
    pub tax_total: f64,
    /// Grand total (tax_basis_total + tax_total).
    pub grand_total: f64,
    /// Amount due.
    pub due_payable: f64,
    /// Payment terms (optional).
    pub payment_terms: Option<PaymentTerms>,
    /// Buyer order reference number (optional).
    pub buyer_reference: Option<String>,
}

/// A trade party (seller or buyer).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeParty {
    /// Legal name.
    pub name: String,
    /// Postal address.
    pub address: Address,
    /// VAT identification number (e.g., "NL123456789B01").
    pub tax_id: Option<String>,
    /// Company registration number (e.g., KvK).
    pub registration_id: Option<String>,
    /// Contact email (optional).
    pub email: Option<String>,
}

/// A postal address.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Address {
    /// Street and house number.
    pub street: Option<String>,
    /// City.
    pub city: Option<String>,
    /// Postal code.
    pub postal_code: Option<String>,
    /// ISO 3166-1 alpha-2 country code (e.g., "NL", "DE").
    pub country_code: String,
}

/// A single invoice line item.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LineItem {
    /// Line item identifier (e.g., "1", "2").
    pub id: String,
    /// Product/service description.
    pub description: String,
    /// Quantity.
    pub quantity: f64,
    /// Unit code (UN/ECE Rec 20, e.g., "C62" for pieces, "HUR" for hours).
    pub unit_code: String,
    /// Net unit price (excluding tax).
    pub unit_price: f64,
    /// Line total (quantity * unit_price).
    pub line_total: f64,
    /// Applicable tax rate as percentage (e.g., 21.0 for 21%).
    pub tax_rate: f64,
    /// Tax category code (e.g., "S" for standard rate).
    pub tax_category: TaxCategory,
}

/// VAT/tax category codes per EN 16931.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TaxCategory {
    /// S — Standard rate.
    Standard,
    /// Z — Zero rated.
    Zero,
    /// E — Exempt.
    Exempt,
    /// AE — Reverse charge.
    ReverseCharge,
    /// K — Intra-community supply.
    IntraCommunity,
    /// G — Export outside EU.
    Export,
    /// O — Not subject to VAT.
    NotSubject,
    /// L — Canary Islands IGIC.
    CanaryIslands,
    /// M — Ceuta/Melilla IPSI.
    CeutaMelilla,
}

impl TaxCategory {
    /// UNTDID 5305 code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Standard => "S",
            Self::Zero => "Z",
            Self::Exempt => "E",
            Self::ReverseCharge => "AE",
            Self::IntraCommunity => "K",
            Self::Export => "G",
            Self::NotSubject => "O",
            Self::CanaryIslands => "L",
            Self::CeutaMelilla => "M",
        }
    }

    /// Parse from UNTDID 5305 code string.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "S" => Some(Self::Standard),
            "Z" => Some(Self::Zero),
            "E" => Some(Self::Exempt),
            "AE" => Some(Self::ReverseCharge),
            "K" => Some(Self::IntraCommunity),
            "G" => Some(Self::Export),
            "O" => Some(Self::NotSubject),
            "L" => Some(Self::CanaryIslands),
            "M" => Some(Self::CeutaMelilla),
            _ => None,
        }
    }
}

/// Payment terms.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PaymentTerms {
    /// Human-readable description (e.g., "Net 30 days").
    pub description: Option<String>,
    /// Due date.
    pub due_date: Option<NaiveDate>,
}

impl ZugferdInvoice {
    /// Validate the invoice against its declared profile.
    ///
    /// Returns a list of validation issues (empty = valid).
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.invoice_number.is_empty() {
            issues.push("invoice_number is required".into());
        }
        if self.currency.len() != 3 {
            issues.push(format!(
                "currency must be 3-letter ISO 4217 code, got '{}'",
                self.currency
            ));
        }
        if self.seller.name.is_empty() {
            issues.push("seller.name is required".into());
        }
        if self.buyer.name.is_empty() {
            issues.push("buyer.name is required".into());
        }

        if self.profile.requires_line_items() && self.line_items.is_empty() {
            issues.push(format!(
                "profile {:?} requires at least one line item",
                self.profile
            ));
        }

        if matches!(
            self.profile,
            ZugferdProfile::EN16931 | ZugferdProfile::Extended
        ) {
            if self.seller.tax_id.is_none() {
                issues.push("seller.tax_id is required for EN16931/Extended".into());
            }
            if self.seller.address.country_code.len() != 2 {
                issues.push("seller.address.country_code must be ISO 3166-1 alpha-2".into());
            }
            if self.buyer.address.country_code.len() != 2 {
                issues.push("buyer.address.country_code must be ISO 3166-1 alpha-2".into());
            }
        }

        issues
    }

    /// Generate CII XML conforming to the invoice's profile.
    pub fn to_xml(&self) -> Result<String> {
        let issues = self.validate();
        if !issues.is_empty() {
            return Err(InvoiceError::ProfileValidation(issues.join("; ")));
        }

        let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

        // XML declaration
        writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
            .map_err(|e| InvoiceError::Xml(e.to_string()))?;

        // Root element with namespaces
        let mut root = BytesStart::new("rsm:CrossIndustryInvoice");
        root.push_attribute(("xmlns:rsm", NS_RSM));
        root.push_attribute(("xmlns:ram", NS_RAM));
        root.push_attribute(("xmlns:qdt", NS_QDT));
        root.push_attribute(("xmlns:udt", NS_UDT));
        w_start(&mut writer, root)?;

        self.write_context(&mut writer)?;
        self.write_document(&mut writer)?;
        self.write_transaction(&mut writer)?;

        w_end(&mut writer, "rsm:CrossIndustryInvoice")?;

        let buf = writer.into_inner().into_inner();
        String::from_utf8(buf).map_err(|e| InvoiceError::Xml(e.to_string()))
    }

    /// Parse a ZUGFeRD/Factur-X invoice from CII XML.
    pub fn from_xml(xml: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(xml)
            .map_err(|e| InvoiceError::Xml(format!("CII parse: {e}")))?;

        let root = doc.root_element();

        // ExchangedDocumentContext → profile
        let profile = root
            .descendants()
            .find(|n| n.has_tag_name("GuidelineSpecifiedDocumentContextParameter"))
            .and_then(|n| {
                n.descendants()
                    .find(|c| c.has_tag_name("ID"))
                    .and_then(|c| c.text())
            })
            .and_then(parse_profile_urn)
            .unwrap_or(ZugferdProfile::Minimum);

        // ExchangedDocument
        let root_opt = Some(root);
        let exch_doc = find_descendant(&root_opt, "ExchangedDocument");
        let invoice_number = text_of(&exch_doc, "ID").unwrap_or_default();
        let type_code = text_of(&exch_doc, "TypeCode").unwrap_or_else(|| "380".into());
        let issue_date = date_of(&exch_doc, "IssueDateTime");

        // SupplyChainTradeTransaction
        let transaction = find_descendant(&root_opt, "SupplyChainTradeTransaction");

        // Trade agreement
        let agreement = find_descendant(&transaction, "ApplicableHeaderTradeAgreement");
        let seller = parse_trade_party(&agreement, "SellerTradeParty");
        let buyer = parse_trade_party(&agreement, "BuyerTradeParty");
        let buyer_reference = text_of(&agreement, "BuyerReference");

        // Line items
        let line_items: Vec<LineItem> = transaction
            .map(|t| {
                t.children()
                    .filter(|n| n.has_tag_name("IncludedSupplyChainTradeLineItem"))
                    .filter_map(parse_line_item)
                    .collect()
            })
            .unwrap_or_default();

        // Settlement
        let settlement = find_descendant(&transaction, "ApplicableHeaderTradeSettlement");
        let currency = text_of(&settlement, "InvoiceCurrencyCode").unwrap_or_else(|| "EUR".into());

        let monetary = find_descendant(
            &settlement,
            "SpecifiedTradeSettlementHeaderMonetarySummation",
        );
        let tax_basis_total = float_of(&monetary, "TaxBasisTotalAmount");
        let tax_total = float_of(&monetary, "TaxTotalAmount");
        let grand_total = float_of(&monetary, "GrandTotalAmount");
        let due_payable = float_of(&monetary, "DuePayableAmount");

        let payment_terms =
            find_descendant(&settlement, "SpecifiedTradePaymentTerms").map(|pt| PaymentTerms {
                description: text_of(&Some(pt), "Description"),
                due_date: date_of(&Some(pt), "DueDateDateTime"),
            });

        Ok(ZugferdInvoice {
            profile,
            invoice_number,
            type_code,
            issue_date: issue_date.unwrap_or_else(|| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
            seller: seller.unwrap_or_else(default_party),
            buyer: buyer.unwrap_or_else(default_party),
            line_items,
            currency,
            tax_basis_total,
            tax_total,
            grand_total,
            due_payable,
            payment_terms,
            buyer_reference,
        })
    }

    // -- XML writing helpers --

    fn write_context(&self, w: &mut Writer<Cursor<Vec<u8>>>) -> Result<()> {
        w_start(w, BytesStart::new("rsm:ExchangedDocumentContext"))?;
        w_start(
            w,
            BytesStart::new("ram:GuidelineSpecifiedDocumentContextParameter"),
        )?;
        w_text_elem(w, "ram:ID", self.profile.urn())?;
        w_end(w, "ram:GuidelineSpecifiedDocumentContextParameter")?;
        w_end(w, "rsm:ExchangedDocumentContext")
    }

    fn write_document(&self, w: &mut Writer<Cursor<Vec<u8>>>) -> Result<()> {
        w_start(w, BytesStart::new("rsm:ExchangedDocument"))?;
        w_text_elem(w, "ram:ID", &self.invoice_number)?;
        w_text_elem(w, "ram:TypeCode", &self.type_code)?;
        write_date(w, "ram:IssueDateTime", self.issue_date)?;
        w_end(w, "rsm:ExchangedDocument")
    }

    fn write_transaction(&self, w: &mut Writer<Cursor<Vec<u8>>>) -> Result<()> {
        w_start(w, BytesStart::new("rsm:SupplyChainTradeTransaction"))?;

        // Line items
        for item in &self.line_items {
            write_line_item(w, item, &self.currency)?;
        }

        // Trade agreement
        w_start(w, BytesStart::new("ram:ApplicableHeaderTradeAgreement"))?;
        if let Some(ref br) = self.buyer_reference {
            w_text_elem(w, "ram:BuyerReference", br)?;
        }
        write_trade_party(w, "ram:SellerTradeParty", &self.seller)?;
        write_trade_party(w, "ram:BuyerTradeParty", &self.buyer)?;
        w_end(w, "ram:ApplicableHeaderTradeAgreement")?;

        // Trade delivery
        w_start(w, BytesStart::new("ram:ApplicableHeaderTradeDelivery"))?;
        w_end(w, "ram:ApplicableHeaderTradeDelivery")?;

        // Trade settlement
        w_start(w, BytesStart::new("ram:ApplicableHeaderTradeSettlement"))?;
        w_text_elem(w, "ram:InvoiceCurrencyCode", &self.currency)?;

        if let Some(ref pt) = self.payment_terms {
            w_start(w, BytesStart::new("ram:SpecifiedTradePaymentTerms"))?;
            if let Some(ref desc) = pt.description {
                w_text_elem(w, "ram:Description", desc)?;
            }
            if let Some(due) = pt.due_date {
                write_date(w, "ram:DueDateDateTime", due)?;
            }
            w_end(w, "ram:SpecifiedTradePaymentTerms")?;
        }

        // Tax breakdown (aggregate from line items or use totals)
        self.write_tax_summary(w)?;

        // Monetary summation
        w_start(
            w,
            BytesStart::new("ram:SpecifiedTradeSettlementHeaderMonetarySummation"),
        )?;
        w_amount(
            w,
            "ram:LineTotalAmount",
            self.tax_basis_total,
            &self.currency,
        )?;
        w_amount(
            w,
            "ram:TaxBasisTotalAmount",
            self.tax_basis_total,
            &self.currency,
        )?;
        w_amount(w, "ram:TaxTotalAmount", self.tax_total, &self.currency)?;
        w_amount(w, "ram:GrandTotalAmount", self.grand_total, &self.currency)?;
        w_amount(w, "ram:DuePayableAmount", self.due_payable, &self.currency)?;
        w_end(w, "ram:SpecifiedTradeSettlementHeaderMonetarySummation")?;

        w_end(w, "ram:ApplicableHeaderTradeSettlement")?;
        w_end(w, "rsm:SupplyChainTradeTransaction")
    }

    fn write_tax_summary(&self, w: &mut Writer<Cursor<Vec<u8>>>) -> Result<()> {
        // Group line items by (tax_category, tax_rate).
        let mut groups: std::collections::BTreeMap<(String, i64), (f64, f64)> =
            std::collections::BTreeMap::new();

        if self.line_items.is_empty() {
            // Minimum/BasicWL: single summary entry.
            let key = ("S".to_string(), 0);
            groups.insert(key, (self.tax_basis_total, self.tax_total));
        } else {
            for item in &self.line_items {
                let rate_key = (item.tax_rate * 100.0) as i64; // avoid float key
                let key = (item.tax_category.code().to_string(), rate_key);
                let entry = groups.entry(key).or_insert((0.0, 0.0));
                entry.0 += item.line_total;
                entry.1 += item.line_total * item.tax_rate / 100.0;
            }
        }

        for ((cat, rate_key), (basis, tax)) in &groups {
            w_start(w, BytesStart::new("ram:ApplicableTradeTax"))?;
            w_amount(w, "ram:CalculatedAmount", *tax, &self.currency)?;
            w_text_elem(w, "ram:TypeCode", "VAT")?;
            w_amount(w, "ram:BasisAmount", *basis, &self.currency)?;
            w_text_elem(w, "ram:CategoryCode", cat)?;
            let rate = *rate_key as f64 / 100.0;
            w_text_elem(w, "ram:RateApplicablePercent", &format_amount(rate))?;
            w_end(w, "ram:ApplicableTradeTax")?;
        }

        Ok(())
    }
}

// -- XML writing utilities --

type XmlWriter = Writer<Cursor<Vec<u8>>>;

fn w_start(w: &mut XmlWriter, elem: BytesStart<'_>) -> Result<()> {
    w.write_event(Event::Start(elem))
        .map_err(|e| InvoiceError::Xml(e.to_string()))
}

fn w_end(w: &mut XmlWriter, name: &str) -> Result<()> {
    w.write_event(Event::End(BytesEnd::new(name)))
        .map_err(|e| InvoiceError::Xml(e.to_string()))
}

fn w_text_elem(w: &mut XmlWriter, name: &str, text: &str) -> Result<()> {
    w_start(w, BytesStart::new(name))?;
    w.write_event(Event::Text(BytesText::new(text)))
        .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w_end(w, name)
}

fn w_amount(w: &mut XmlWriter, name: &str, amount: f64, currency: &str) -> Result<()> {
    let mut elem = BytesStart::new(name);
    elem.push_attribute(("currencyID", currency));
    w.write_event(Event::Start(elem))
        .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w.write_event(Event::Text(BytesText::new(&format_amount(amount))))
        .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w_end(w, name)
}

fn write_date(w: &mut XmlWriter, name: &str, date: NaiveDate) -> Result<()> {
    w_start(w, BytesStart::new(name))?;
    let mut ds = BytesStart::new("udt:DateTimeString");
    ds.push_attribute(("format", "102"));
    w.write_event(Event::Start(ds))
        .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w.write_event(Event::Text(BytesText::new(
        &date.format("%Y%m%d").to_string(),
    )))
    .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w_end(w, "udt:DateTimeString")?;
    w_end(w, name)
}

fn write_trade_party(w: &mut XmlWriter, name: &str, party: &TradeParty) -> Result<()> {
    w_start(w, BytesStart::new(name))?;
    w_text_elem(w, "ram:Name", &party.name)?;

    if let Some(ref reg) = party.registration_id {
        w_start(w, BytesStart::new("ram:SpecifiedLegalOrganization"))?;
        w_text_elem(w, "ram:ID", reg)?;
        w_end(w, "ram:SpecifiedLegalOrganization")?;
    }

    if let Some(ref email) = party.email {
        w_start(w, BytesStart::new("ram:DefinedTradeContact"))?;
        w_start(w, BytesStart::new("ram:EmailURIUniversalCommunication"))?;
        w_text_elem(w, "ram:URIID", email)?;
        w_end(w, "ram:EmailURIUniversalCommunication")?;
        w_end(w, "ram:DefinedTradeContact")?;
    }

    // Postal address
    w_start(w, BytesStart::new("ram:PostalTradeAddress"))?;
    if let Some(ref pc) = party.address.postal_code {
        w_text_elem(w, "ram:PostcodeCode", pc)?;
    }
    if let Some(ref street) = party.address.street {
        w_text_elem(w, "ram:LineOne", street)?;
    }
    if let Some(ref city) = party.address.city {
        w_text_elem(w, "ram:CityName", city)?;
    }
    w_text_elem(w, "ram:CountryID", &party.address.country_code)?;
    w_end(w, "ram:PostalTradeAddress")?;

    // Tax registration
    if let Some(ref tax_id) = party.tax_id {
        w_start(w, BytesStart::new("ram:SpecifiedTaxRegistration"))?;
        let mut id_elem = BytesStart::new("ram:ID");
        id_elem.push_attribute(("schemeID", "VA"));
        w.write_event(Event::Start(id_elem))
            .map_err(|e| InvoiceError::Xml(e.to_string()))?;
        w.write_event(Event::Text(BytesText::new(tax_id)))
            .map_err(|e| InvoiceError::Xml(e.to_string()))?;
        w_end(w, "ram:ID")?;
        w_end(w, "ram:SpecifiedTaxRegistration")?;
    }

    w_end(w, name)
}

fn write_line_item(w: &mut XmlWriter, item: &LineItem, currency: &str) -> Result<()> {
    w_start(w, BytesStart::new("ram:IncludedSupplyChainTradeLineItem"))?;

    // Line document
    w_start(w, BytesStart::new("ram:AssociatedDocumentLineDocument"))?;
    w_text_elem(w, "ram:LineID", &item.id)?;
    w_end(w, "ram:AssociatedDocumentLineDocument")?;

    // Product
    w_start(w, BytesStart::new("ram:SpecifiedTradeProduct"))?;
    w_text_elem(w, "ram:Name", &item.description)?;
    w_end(w, "ram:SpecifiedTradeProduct")?;

    // Line agreement (price)
    w_start(w, BytesStart::new("ram:SpecifiedLineTradeAgreement"))?;
    w_start(w, BytesStart::new("ram:NetPriceProductTradePrice"))?;
    w_amount(w, "ram:ChargeAmount", item.unit_price, currency)?;
    w_end(w, "ram:NetPriceProductTradePrice")?;
    w_end(w, "ram:SpecifiedLineTradeAgreement")?;

    // Line delivery (quantity)
    w_start(w, BytesStart::new("ram:SpecifiedLineTradeDelivery"))?;
    let mut qty_elem = BytesStart::new("ram:BilledQuantity");
    qty_elem.push_attribute(("unitCode", item.unit_code.as_str()));
    w.write_event(Event::Start(qty_elem))
        .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w.write_event(Event::Text(BytesText::new(&format_amount(item.quantity))))
        .map_err(|e| InvoiceError::Xml(e.to_string()))?;
    w_end(w, "ram:BilledQuantity")?;
    w_end(w, "ram:SpecifiedLineTradeDelivery")?;

    // Line settlement (tax + total)
    w_start(w, BytesStart::new("ram:SpecifiedLineTradeSettlement"))?;
    w_start(w, BytesStart::new("ram:ApplicableTradeTax"))?;
    w_text_elem(w, "ram:TypeCode", "VAT")?;
    w_text_elem(w, "ram:CategoryCode", item.tax_category.code())?;
    w_text_elem(
        w,
        "ram:RateApplicablePercent",
        &format_amount(item.tax_rate),
    )?;
    w_end(w, "ram:ApplicableTradeTax")?;
    w_start(
        w,
        BytesStart::new("ram:SpecifiedTradeSettlementLineMonetarySummation"),
    )?;
    w_amount(w, "ram:LineTotalAmount", item.line_total, currency)?;
    w_end(w, "ram:SpecifiedTradeSettlementLineMonetarySummation")?;
    w_end(w, "ram:SpecifiedLineTradeSettlement")?;

    w_end(w, "ram:IncludedSupplyChainTradeLineItem")
}

fn format_amount(v: f64) -> String {
    // CII amounts use 2 decimal places for currency, more for rates.
    if (v - v.round()).abs() < 1e-9 {
        format!("{v:.2}")
    } else {
        // Trim trailing zeroes but keep at least 2 decimals.
        let s = format!("{v:.4}");
        let trimmed = s.trim_end_matches('0');
        let dot_pos = trimmed.find('.').unwrap_or(trimmed.len());
        let decimals = trimmed.len() - dot_pos - 1;
        if decimals < 2 {
            format!("{v:.2}")
        } else {
            trimmed.to_string()
        }
    }
}

// -- XML parsing utilities --

fn find_descendant<'a>(
    parent: &Option<roxmltree::Node<'a, 'a>>,
    tag: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    parent.and_then(|p| p.descendants().find(|n| n.has_tag_name(tag)))
}

fn text_of(parent: &Option<roxmltree::Node>, tag: &str) -> Option<String> {
    parent.and_then(|p| {
        p.descendants()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text().map(String::from))
    })
}

fn float_of(parent: &Option<roxmltree::Node>, tag: &str) -> f64 {
    text_of(parent, tag)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn date_of(parent: &Option<roxmltree::Node>, tag: &str) -> Option<NaiveDate> {
    let node = parent.and_then(|p| p.descendants().find(|n| n.has_tag_name(tag)))?;
    let text = node
        .descendants()
        .find(|n| n.has_tag_name("DateTimeString"))
        .and_then(|n| n.text())
        .or_else(|| node.text())?;

    NaiveDate::parse_from_str(text.trim(), "%Y%m%d").ok()
}

fn parse_profile_urn(urn: &str) -> Option<ZugferdProfile> {
    let lower = urn.to_lowercase();
    if lower.contains("minimum") {
        Some(ZugferdProfile::Minimum)
    } else if lower.contains("basicwl") {
        Some(ZugferdProfile::BasicWL)
    } else if lower.contains("basic") {
        Some(ZugferdProfile::Basic)
    } else if lower.contains("en16931") || lower.contains("cen.eu") {
        Some(ZugferdProfile::EN16931)
    } else if lower.contains("extended") {
        Some(ZugferdProfile::Extended)
    } else {
        None
    }
}

fn parse_trade_party(agreement: &Option<roxmltree::Node>, tag: &str) -> Option<TradeParty> {
    let node = agreement.and_then(|a| a.descendants().find(|n| n.has_tag_name(tag)))?;
    let parent = Some(node);

    let name = text_of(&parent, "Name")?;

    let address_node = node
        .descendants()
        .find(|n| n.has_tag_name("PostalTradeAddress"));
    let addr_parent = address_node;
    let address = Address {
        street: addr_parent.and_then(|a| {
            a.descendants()
                .find(|n| n.has_tag_name("LineOne"))
                .and_then(|n| n.text().map(String::from))
        }),
        city: addr_parent.and_then(|a| {
            a.descendants()
                .find(|n| n.has_tag_name("CityName"))
                .and_then(|n| n.text().map(String::from))
        }),
        postal_code: addr_parent.and_then(|a| {
            a.descendants()
                .find(|n| n.has_tag_name("PostcodeCode"))
                .and_then(|n| n.text().map(String::from))
        }),
        country_code: addr_parent
            .and_then(|a| {
                a.descendants()
                    .find(|n| n.has_tag_name("CountryID"))
                    .and_then(|n| n.text().map(String::from))
            })
            .unwrap_or_default(),
    };

    let tax_id = node
        .descendants()
        .find(|n| n.has_tag_name("SpecifiedTaxRegistration"))
        .and_then(|n| {
            n.descendants()
                .find(|c| c.has_tag_name("ID"))
                .and_then(|c| c.text().map(String::from))
        });

    let registration_id = node
        .descendants()
        .find(|n| n.has_tag_name("SpecifiedLegalOrganization"))
        .and_then(|n| {
            n.descendants()
                .find(|c| c.has_tag_name("ID"))
                .and_then(|c| c.text().map(String::from))
        });

    let email = node
        .descendants()
        .find(|n| n.has_tag_name("EmailURIUniversalCommunication"))
        .and_then(|n| {
            n.descendants()
                .find(|c| c.has_tag_name("URIID"))
                .and_then(|c| c.text().map(String::from))
        });

    Some(TradeParty {
        name,
        address,
        tax_id,
        registration_id,
        email,
    })
}

fn parse_line_item(node: roxmltree::Node) -> Option<LineItem> {
    let parent = Some(node);

    let id = text_of(&parent, "LineID").unwrap_or_else(|| "1".into());
    let description = text_of(&parent, "Name").unwrap_or_default();

    let qty_node = node
        .descendants()
        .find(|n| n.has_tag_name("BilledQuantity"));
    let quantity = qty_node
        .and_then(|n| n.text().and_then(|t| t.trim().parse::<f64>().ok()))
        .unwrap_or(1.0);
    let unit_code = qty_node
        .and_then(|n| n.attribute("unitCode").map(String::from))
        .unwrap_or_else(|| "C62".into());

    let unit_price = float_of(&parent, "ChargeAmount");
    let line_total = float_of(&parent, "LineTotalAmount");

    let tax_rate = float_of(&parent, "RateApplicablePercent");
    let tax_category = text_of(&parent, "CategoryCode")
        .and_then(|c| TaxCategory::from_code(&c))
        .unwrap_or(TaxCategory::Standard);

    Some(LineItem {
        id,
        description,
        quantity,
        unit_code,
        unit_price,
        line_total,
        tax_rate,
        tax_category,
    })
}

fn default_party() -> TradeParty {
    TradeParty {
        name: String::new(),
        address: Address {
            street: None,
            city: None,
            postal_code: None,
            country_code: String::new(),
        },
        tax_id: None,
        registration_id: None,
        email: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_invoice() -> ZugferdInvoice {
        ZugferdInvoice {
            profile: ZugferdProfile::EN16931,
            invoice_number: "INV-2026-001".into(),
            type_code: "380".into(),
            issue_date: NaiveDate::from_ymd_opt(2026, 3, 6).unwrap(),
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
                name: "Acme GmbH".into(),
                address: Address {
                    street: Some("Hauptstr. 42".into()),
                    city: Some("Berlin".into()),
                    postal_code: Some("10115".into()),
                    country_code: "DE".into(),
                },
                tax_id: Some("DE987654321".into()),
                registration_id: None,
                email: None,
            },
            line_items: vec![
                LineItem {
                    id: "1".into(),
                    description: "PDF Engine License".into(),
                    quantity: 1.0,
                    unit_code: "C62".into(),
                    unit_price: 5000.0,
                    line_total: 5000.0,
                    tax_rate: 21.0,
                    tax_category: TaxCategory::Standard,
                },
                LineItem {
                    id: "2".into(),
                    description: "Support (annual)".into(),
                    quantity: 12.0,
                    unit_code: "MON".into(),
                    unit_price: 200.0,
                    line_total: 2400.0,
                    tax_rate: 21.0,
                    tax_category: TaxCategory::Standard,
                },
            ],
            currency: "EUR".into(),
            tax_basis_total: 7400.0,
            tax_total: 1554.0,
            grand_total: 8954.0,
            due_payable: 8954.0,
            payment_terms: Some(PaymentTerms {
                description: Some("Net 30 days".into()),
                due_date: Some(NaiveDate::from_ymd_opt(2026, 4, 5).unwrap()),
            }),
            buyer_reference: Some("PO-2026-042".into()),
        }
    }

    #[test]
    fn xml_roundtrip() {
        let invoice = sample_invoice();
        let xml = invoice.to_xml().unwrap();

        assert!(xml.contains("CrossIndustryInvoice"));
        assert!(xml.contains("INV-2026-001"));
        assert!(xml.contains("XFA Solutions B.V."));
        assert!(xml.contains("Acme GmbH"));
        assert!(xml.contains("NL123456789B01"));
        assert!(xml.contains("20260306"));

        let parsed = ZugferdInvoice::from_xml(&xml).unwrap();
        assert_eq!(parsed.profile, ZugferdProfile::EN16931);
        assert_eq!(parsed.invoice_number, "INV-2026-001");
        assert_eq!(parsed.type_code, "380");
        assert_eq!(
            parsed.issue_date,
            NaiveDate::from_ymd_opt(2026, 3, 6).unwrap()
        );
        assert_eq!(parsed.seller.name, "XFA Solutions B.V.");
        assert_eq!(parsed.buyer.name, "Acme GmbH");
        assert_eq!(parsed.seller.tax_id.as_deref(), Some("NL123456789B01"));
        assert_eq!(parsed.currency, "EUR");
        assert_eq!(parsed.line_items.len(), 2);
        assert_eq!(parsed.line_items[0].description, "PDF Engine License");
        assert!((parsed.grand_total - 8954.0).abs() < 0.01);
    }

    #[test]
    fn minimum_profile() {
        let inv = ZugferdInvoice {
            profile: ZugferdProfile::Minimum,
            invoice_number: "MIN-001".into(),
            type_code: "380".into(),
            issue_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            seller: TradeParty {
                name: "Seller".into(),
                address: Address {
                    street: None,
                    city: None,
                    postal_code: None,
                    country_code: "NL".into(),
                },
                tax_id: None,
                registration_id: None,
                email: None,
            },
            buyer: TradeParty {
                name: "Buyer".into(),
                address: Address {
                    street: None,
                    city: None,
                    postal_code: None,
                    country_code: "DE".into(),
                },
                tax_id: None,
                registration_id: None,
                email: None,
            },
            line_items: vec![],
            currency: "EUR".into(),
            tax_basis_total: 100.0,
            tax_total: 21.0,
            grand_total: 121.0,
            due_payable: 121.0,
            payment_terms: None,
            buyer_reference: None,
        };

        let xml = inv.to_xml().unwrap();
        assert!(xml.contains("urn:factur-x.eu:1p0:minimum"));
        let parsed = ZugferdInvoice::from_xml(&xml).unwrap();
        assert_eq!(parsed.profile, ZugferdProfile::Minimum);
        assert!(parsed.line_items.is_empty());
    }

    #[test]
    fn validation_catches_missing_fields() {
        let inv = ZugferdInvoice {
            profile: ZugferdProfile::EN16931,
            invoice_number: String::new(), // missing
            type_code: "380".into(),
            issue_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            seller: TradeParty {
                name: String::new(), // missing
                address: Address {
                    street: None,
                    city: None,
                    postal_code: None,
                    country_code: "NL".into(),
                },
                tax_id: None, // missing for EN16931
                registration_id: None,
                email: None,
            },
            buyer: TradeParty {
                name: "Buyer".into(),
                address: Address {
                    street: None,
                    city: None,
                    postal_code: None,
                    country_code: "DE".into(),
                },
                tax_id: None,
                registration_id: None,
                email: None,
            },
            line_items: vec![], // missing for EN16931
            currency: "EUR".into(),
            tax_basis_total: 0.0,
            tax_total: 0.0,
            grand_total: 0.0,
            due_payable: 0.0,
            payment_terms: None,
            buyer_reference: None,
        };

        let issues = inv.validate();
        assert!(issues.iter().any(|i| i.contains("invoice_number")));
        assert!(issues.iter().any(|i| i.contains("seller.name")));
        assert!(issues.iter().any(|i| i.contains("seller.tax_id")));
        assert!(issues.iter().any(|i| i.contains("line item")));
    }

    #[test]
    fn tax_category_roundtrip() {
        for cat in [
            TaxCategory::Standard,
            TaxCategory::Zero,
            TaxCategory::Exempt,
            TaxCategory::ReverseCharge,
            TaxCategory::IntraCommunity,
            TaxCategory::Export,
            TaxCategory::NotSubject,
        ] {
            let code = cat.code();
            assert_eq!(TaxCategory::from_code(code), Some(cat));
        }
    }

    #[test]
    fn profile_urn_values() {
        assert!(ZugferdProfile::EN16931.urn().contains("en16931"));
        assert!(ZugferdProfile::Basic.urn().contains("basic"));
        assert!(!ZugferdProfile::Basic.urn().contains("basicwl"));
        assert!(ZugferdProfile::BasicWL.urn().contains("basicwl"));
    }

    #[test]
    fn format_amount_precision() {
        assert_eq!(format_amount(100.0), "100.00");
        assert_eq!(format_amount(99.99), "99.99");
        assert_eq!(format_amount(21.0), "21.00");
        assert_eq!(format_amount(19.5), "19.50");
    }

    #[test]
    fn credit_note_type_code() {
        let inv = ZugferdInvoice {
            profile: ZugferdProfile::Minimum,
            invoice_number: "CN-001".into(),
            type_code: "381".into(),
            issue_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            seller: TradeParty {
                name: "Seller".into(),
                address: Address {
                    street: None,
                    city: None,
                    postal_code: None,
                    country_code: "NL".into(),
                },
                tax_id: None,
                registration_id: None,
                email: None,
            },
            buyer: TradeParty {
                name: "Buyer".into(),
                address: Address {
                    street: None,
                    city: None,
                    postal_code: None,
                    country_code: "DE".into(),
                },
                tax_id: None,
                registration_id: None,
                email: None,
            },
            line_items: vec![],
            currency: "EUR".into(),
            tax_basis_total: -50.0,
            tax_total: -10.5,
            grand_total: -60.5,
            due_payable: -60.5,
            payment_terms: None,
            buyer_reference: None,
        };
        let xml = inv.to_xml().unwrap();
        assert!(xml.contains("<ram:TypeCode>381</ram:TypeCode>"));
    }
}
