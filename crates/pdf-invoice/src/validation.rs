//! EN 16931 business rule validation for ZUGFeRD/Factur-X invoices.
//!
//! This validator focuses on the core structural and arithmetic rules that can
//! be enforced from the current invoice data model.

use crate::iso_codes::{is_valid_country, is_valid_currency};
use crate::zugferd::{TaxCategory, ZugferdInvoice, ZugferdProfile};
use chrono::NaiveDate;

const AMOUNT_TOLERANCE: f64 = 0.01;

/// Severity of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The invoice violates a mandatory business rule.
    Error,
    /// The invoice has a potential issue that should be reviewed.
    Warning,
}

/// A single validation issue.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Business rule identifier (e.g., "BR-CO-10").
    pub rule: String,
    /// Human-readable description.
    pub message: String,
    /// Severity level.
    pub severity: Severity,
}

/// Backward-compatible validation report used by the existing public API.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// All issues found during validation.
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Whether the invoice passes all checks (no errors).
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }

    /// Number of errors.
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .count()
    }

    /// Number of warnings.
    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
            .count()
    }
}

/// Structured EN 16931 validation result.
#[derive(Debug, Clone, Default)]
pub struct En16931ValidationResult {
    /// Rules that passed.
    pub passed: Vec<String>,
    /// Rules that failed with a diagnostic message.
    pub failed: Vec<(String, String)>,
    /// Advisory diagnostics and model gaps.
    pub warnings: Vec<(String, String)>,
}

impl En16931ValidationResult {
    /// Whether the invoice passes all mandatory checks.
    pub fn is_valid(&self) -> bool {
        self.failed.is_empty()
    }

    /// Number of failed checks.
    pub fn error_count(&self) -> usize {
        self.failed.len()
    }

    /// Number of warnings.
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }

    fn pass(&mut self, rule: &str) {
        if !self.passed.iter().any(|entry| entry == rule) {
            self.passed.push(rule.to_string());
        }
    }

    fn fail(&mut self, rule: &str, message: impl Into<String>) {
        self.failed.push((rule.to_string(), message.into()));
    }

    fn warn(&mut self, rule: &str, message: impl Into<String>) {
        self.warnings.push((rule.to_string(), message.into()));
    }
}

impl From<En16931ValidationResult> for ValidationReport {
    fn from(result: En16931ValidationResult) -> Self {
        let mut issues = Vec::with_capacity(result.failed.len() + result.warnings.len());

        for (rule, message) in result.failed {
            issues.push(ValidationIssue {
                rule,
                message,
                severity: Severity::Error,
            });
        }

        for (rule, message) in result.warnings {
            issues.push(ValidationIssue {
                rule,
                message,
                severity: Severity::Warning,
            });
        }

        ValidationReport { issues }
    }
}

/// Validate an invoice against the implemented EN 16931 business rules.
pub fn validate_en16931(invoice: &ZugferdInvoice) -> En16931ValidationResult {
    let mut result = En16931ValidationResult::default();

    if !matches!(
        invoice.profile,
        ZugferdProfile::EN16931 | ZugferdProfile::Extended
    ) {
        result.warn(
            "PROFILE-01",
            format!(
                "Invoice profile {:?} is not EN16931/XRechnung; EN 16931 results are advisory",
                invoice.profile
            ),
        );
    }

    check_br_01(invoice, &mut result);
    check_br_02(invoice, &mut result);
    check_br_03(invoice, &mut result);
    check_br_04(invoice, &mut result);
    check_br_05(invoice, &mut result);
    check_br_06(invoice, &mut result);
    check_br_07(invoice, &mut result);
    check_br_08(invoice, &mut result);
    check_br_cl_01(invoice, &mut result);
    check_br_cl_04(invoice, &mut result);
    check_br_co_10(invoice, &mut result);
    check_br_co_13(invoice, &mut result);
    check_br_co_15(invoice, &mut result);
    check_br_s_08(invoice, &mut result);

    result
}

/// Backward-compatible validation entry point.
pub fn validate_invoice(invoice: &ZugferdInvoice) -> ValidationReport {
    validate_en16931(invoice).into()
}

fn check_br_01(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    if invoice.invoice_number.trim().is_empty() {
        result.fail("BR-01", "Invoice shall have invoice number");
    } else {
        result.pass("BR-01");
    }
}

fn check_br_02(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    if invoice.issue_date == NaiveDate::MIN {
        result.fail("BR-02", "Invoice shall have issue date");
    } else {
        result.pass("BR-02");
    }
}

fn check_br_03(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    if has_text(invoice.buyer_reference.as_deref()) {
        result.pass("BR-03");
    } else {
        result.fail(
            "BR-03",
            "Invoice shall have buyer reference or order reference; the current model only exposes buyer_reference",
        );
    }
}

fn check_br_04(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    if invoice.seller.name.trim().is_empty() {
        result.fail("BR-04", "Invoice shall have seller name");
    } else {
        result.pass("BR-04");
    }
}

fn check_br_05(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    if invoice.buyer.name.trim().is_empty() {
        result.fail("BR-05", "Invoice shall have buyer name");
    } else {
        result.pass("BR-05");
    }
}

fn check_br_06(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let seller = &invoice.seller.address;
    let mut missing_required = Vec::new();

    if !has_text(seller.city.as_deref()) {
        missing_required.push("city");
    }
    if seller.country_code.trim().is_empty() {
        missing_required.push("country");
    }

    if missing_required.is_empty() {
        result.pass("BR-06");
        if !has_text(seller.street.as_deref()) {
            result.warn(
                "BR-06",
                "Seller street is not populated; KoSIT minimal examples omit the street line",
            );
        }
    } else {
        result.fail(
            "BR-06",
            format!(
                "Invoice shall have seller address (missing {})",
                missing_required.join(", ")
            ),
        );
    }
}

fn check_br_07(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let buyer = &invoice.buyer.address;
    let mut missing_required = Vec::new();

    if !has_text(buyer.city.as_deref()) {
        missing_required.push("city");
    }
    if buyer.country_code.trim().is_empty() {
        missing_required.push("country");
    }

    if missing_required.is_empty() {
        result.pass("BR-07");
        if !has_text(buyer.street.as_deref()) {
            result.warn(
                "BR-07",
                "Buyer street is not populated; KoSIT minimal examples omit the street line",
            );
        }
    } else {
        result.fail(
            "BR-07",
            format!(
                "Invoice shall have buyer address (missing {})",
                missing_required.join(", ")
            ),
        );
    }
}

fn check_br_08(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    match invoice.payment_means.as_ref() {
        Some(means) if !means.type_code.trim().is_empty() => result.pass("BR-08"),
        Some(_) => result.fail("BR-08", "Invoice payment means shall have a type code"),
        None => result.fail("BR-08", "Invoice shall have payment means"),
    }
}

/// BR-CL-01: Currency code must be a valid ISO 4217 code.
fn check_br_cl_01(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    if is_valid_currency(&invoice.currency) {
        result.pass("BR-CL-01");
    } else {
        result.fail(
            "BR-CL-01",
            format!(
                "Invoice currency code '{}' is not a valid ISO 4217 code",
                invoice.currency
            ),
        );
    }
}

/// BR-CL-04: Country codes must be valid ISO 3166-1 alpha-2 codes.
fn check_br_cl_04(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let mut failures = Vec::new();

    if !is_valid_country(&invoice.seller.address.country_code) {
        failures.push(format!(
            "Seller country code '{}' is not a valid ISO 3166-1 alpha-2 code",
            invoice.seller.address.country_code
        ));
    }
    if !is_valid_country(&invoice.buyer.address.country_code) {
        failures.push(format!(
            "Buyer country code '{}' is not a valid ISO 3166-1 alpha-2 code",
            invoice.buyer.address.country_code
        ));
    }

    if failures.is_empty() {
        result.pass("BR-CL-04");
    } else {
        for failure in failures {
            result.fail("BR-CL-04", failure);
        }
    }
}

/// BR-CO-10: Sum of line net amounts = invoice total net amount.
fn check_br_co_10(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let line_sum: f64 = invoice.line_items.iter().map(|item| item.line_total).sum();
    let expected = line_sum + invoice.charge_total - invoice.allowance_total;
    if approx_eq(expected, invoice.tax_basis_total) {
        result.pass("BR-CO-10");
    } else {
        result.fail(
            "BR-CO-10",
            format!(
                "Line totals ({:.2}) + charges ({:.2}) - allowances ({:.2}) does not match invoice total net amount ({:.2})",
                line_sum, invoice.charge_total, invoice.allowance_total, invoice.tax_basis_total
            ),
        );
    }
}

/// BR-CO-13: Invoice total with VAT = total without VAT + VAT amount.
fn check_br_co_13(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let expected = invoice.tax_basis_total + invoice.tax_total;
    if approx_eq(expected, invoice.grand_total) {
        result.pass("BR-CO-13");
    } else {
        result.fail(
            "BR-CO-13",
            format!(
                "Invoice total with VAT ({:.2}) does not equal invoice total without VAT ({:.2}) + VAT amount ({:.2})",
                invoice.grand_total, invoice.tax_basis_total, invoice.tax_total
            ),
        );
    }
}

/// BR-CO-15: Each line net amount = quantity × unit price.
fn check_br_co_15(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let mut found_failure = false;

    for item in &invoice.line_items {
        if item.price_base_quantity <= 0.0 {
            found_failure = true;
            result.fail(
                "BR-CO-15",
                format!(
                    "Line '{}' has invalid price base quantity {}",
                    item.id, item.price_base_quantity
                ),
            );
            continue;
        }

        let expected = item.quantity * (item.unit_price / item.price_base_quantity);
        if !approx_eq(expected, item.line_total) {
            found_failure = true;
            result.fail(
                "BR-CO-15",
                format!(
                    "Line '{}' net amount {:.2} does not equal quantity {} × (unit price {:.4} / base quantity {:.4})",
                    item.id, item.line_total, item.quantity, item.unit_price, item.price_base_quantity
                ),
            );
        }
    }

    if !found_failure {
        result.pass("BR-CO-15");
    }
}

/// BR-S-08: VAT category rate shall match the VAT category code.
fn check_br_s_08(invoice: &ZugferdInvoice, result: &mut En16931ValidationResult) {
    let mut found_failure = false;

    for item in &invoice.line_items {
        match item.tax_category {
            TaxCategory::Standard if item.tax_rate <= 0.0 => {
                found_failure = true;
                result.fail(
                    "BR-S-08",
                    format!(
                        "Line '{}' uses category 'S' but has non-positive VAT rate {:.2}%",
                        item.id, item.tax_rate
                    ),
                );
            }
            TaxCategory::Zero
            | TaxCategory::Exempt
            | TaxCategory::ReverseCharge
            | TaxCategory::IntraCommunity
            | TaxCategory::Export
            | TaxCategory::NotSubject
                if !approx_eq(item.tax_rate, 0.0) =>
            {
                found_failure = true;
                result.fail(
                    "BR-S-08",
                    format!(
                        "Line '{}' uses category '{}' but VAT rate must be 0.00%, got {:.2}%",
                        item.id,
                        item.tax_category.code(),
                        item.tax_rate
                    ),
                );
            }
            _ => {}
        }
    }

    if !found_failure {
        result.pass("BR-S-08");
    }
}

fn has_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

fn approx_eq(left: f64, right: f64) -> bool {
    ((left - right).abs() * 100.0).round() <= (AMOUNT_TOLERANCE * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zugferd::*;
    use chrono::NaiveDate;

    fn valid_invoice() -> ZugferdInvoice {
        ZugferdInvoice {
            profile: ZugferdProfile::EN16931,
            invoice_number: "INV-001".into(),
            type_code: "380".into(),
            issue_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            seller: TradeParty {
                name: "Seller B.V.".into(),
                address: Address {
                    street: Some("Street 1".into()),
                    city: Some("City".into()),
                    postal_code: Some("1234 AB".into()),
                    country_code: "NL".into(),
                },
                tax_id: Some("NL123456789B01".into()),
                registration_id: None,
                email: None,
            },
            buyer: TradeParty {
                name: "Buyer GmbH".into(),
                address: Address {
                    street: Some("Buyer Street 5".into()),
                    city: Some("Berlin".into()),
                    postal_code: Some("10115".into()),
                    country_code: "DE".into(),
                },
                tax_id: None,
                registration_id: None,
                email: None,
            },
            line_items: vec![LineItem {
                id: "1".into(),
                description: "Service".into(),
                quantity: 10.0,
                unit_code: "C62".into(),
                unit_price: 100.0,
                price_base_quantity: 1.0,
                line_total: 1000.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            }],
            currency: "EUR".into(),
            tax_basis_total: 1000.0,
            tax_total: 210.0,
            grand_total: 1210.0,
            due_payable: 1210.0,
            charge_total: 0.0,
            allowance_total: 0.0,
            payment_means: Some(PaymentMeans {
                type_code: "58".into(),
                information: Some("SEPA credit transfer".into()),
            }),
            payment_terms: Some(PaymentTerms {
                description: Some("Pay within 30 days".into()),
                due_date: Some(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()),
            }),
            buyer_reference: Some("PO-001".into()),
        }
    }

    #[test]
    fn valid_invoice_passes() {
        let result = validate_en16931(&valid_invoice());
        assert!(result.is_valid(), "result: {:?}", result);
        assert!(result.warning_count() == 0, "result: {:?}", result);

        let report = validate_invoice(&valid_invoice());
        assert!(report.is_valid(), "issues: {:?}", report.issues);
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn non_en16931_profiles_are_advisory() {
        let mut invoice = valid_invoice();
        invoice.profile = ZugferdProfile::Basic;
        let result = validate_en16931(&invoice);
        assert!(result.is_valid());
        assert!(result.warnings.iter().any(|(rule, _)| rule == "PROFILE-01"));
    }

    #[test]
    fn br_01_requires_invoice_number() {
        let mut invoice = valid_invoice();
        invoice.invoice_number.clear();
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-01"));
    }

    #[test]
    fn br_03_requires_buyer_reference() {
        let mut invoice = valid_invoice();
        invoice.buyer_reference = None;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-03"));
    }

    #[test]
    fn br_06_warns_when_seller_street_is_missing() {
        let mut invoice = valid_invoice();
        invoice.seller.address.street = None;
        let result = validate_en16931(&invoice);
        assert!(result.is_valid(), "result: {:?}", result);
        assert!(result.warnings.iter().any(|(rule, _)| rule == "BR-06"));
    }

    #[test]
    fn br_07_warns_when_buyer_street_is_missing() {
        let mut invoice = valid_invoice();
        invoice.buyer.address.street = None;
        let result = validate_en16931(&invoice);
        assert!(result.is_valid(), "result: {:?}", result);
        assert!(result.warnings.iter().any(|(rule, _)| rule == "BR-07"));
    }

    #[test]
    fn br_08_requires_payment_means() {
        let mut invoice = valid_invoice();
        invoice.payment_means = None;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-08"));
    }

    #[test]
    fn br_cl_01_invalid_currency() {
        let mut invoice = valid_invoice();
        invoice.currency = "XYZ".into();
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-CL-01"));
    }

    #[test]
    fn br_cl_04_invalid_country() {
        let mut invoice = valid_invoice();
        invoice.seller.address.country_code = "XX".into();
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-CL-04"));
    }

    #[test]
    fn br_co_10_line_sum_mismatch() {
        let mut invoice = valid_invoice();
        invoice.tax_basis_total = 999.0;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-CO-10"));
    }

    #[test]
    fn br_co_10_allows_header_charges_and_allowances() {
        let mut invoice = valid_invoice();
        invoice.charge_total = 20.0;
        invoice.allowance_total = 10.0;
        invoice.tax_basis_total = 1010.0;
        invoice.tax_total = 212.1;
        invoice.grand_total = 1222.1;
        invoice.due_payable = 1222.1;

        let result = validate_en16931(&invoice);
        assert!(result.is_valid(), "result: {:?}", result);
    }

    #[test]
    fn br_co_13_grand_total_mismatch() {
        let mut invoice = valid_invoice();
        invoice.grand_total = 9999.0;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-CO-13"));
    }

    #[test]
    fn br_co_15_line_total_mismatch() {
        let mut invoice = valid_invoice();
        invoice.line_items[0].line_total = 500.0;
        invoice.tax_basis_total = 500.0;
        invoice.tax_total = 105.0;
        invoice.grand_total = 605.0;
        invoice.due_payable = 605.0;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-CO-15"));
    }

    #[test]
    fn br_co_15_respects_price_base_quantity() {
        let mut invoice = valid_invoice();
        invoice.line_items[0].quantity = 31.0;
        invoice.line_items[0].unit_price = 386.52;
        invoice.line_items[0].price_base_quantity = 366.0;
        invoice.line_items[0].line_total = 32.74;
        invoice.tax_basis_total = 32.74;
        invoice.tax_total = 6.88;
        invoice.grand_total = 39.62;
        invoice.due_payable = 39.62;

        let result = validate_en16931(&invoice);
        assert!(result.is_valid(), "result: {:?}", result);
    }

    #[test]
    fn br_s_08_zero_rate_standard() {
        let mut invoice = valid_invoice();
        invoice.line_items[0].tax_rate = 0.0;
        invoice.tax_total = 0.0;
        invoice.grand_total = 1000.0;
        invoice.due_payable = 1000.0;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-S-08"));
    }

    #[test]
    fn zero_rated_items_must_have_zero_rate() {
        let mut invoice = valid_invoice();
        invoice.line_items[0].tax_category = TaxCategory::Zero;
        invoice.line_items[0].tax_rate = 9.0;
        invoice.tax_total = 90.0;
        invoice.grand_total = 1090.0;
        invoice.due_payable = 1090.0;
        let result = validate_en16931(&invoice);
        assert!(result.failed.iter().any(|(rule, _)| rule == "BR-S-08"));
    }

    #[test]
    fn multiple_vat_rates_pass() {
        let mut invoice = valid_invoice();
        invoice.line_items = vec![
            LineItem {
                id: "1".into(),
                description: "Standard".into(),
                quantity: 2.0,
                unit_code: "C62".into(),
                unit_price: 100.0,
                price_base_quantity: 1.0,
                line_total: 200.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            },
            LineItem {
                id: "2".into(),
                description: "Reduced".into(),
                quantity: 4.0,
                unit_code: "C62".into(),
                unit_price: 50.0,
                price_base_quantity: 1.0,
                line_total: 200.0,
                tax_rate: 0.0,
                tax_category: TaxCategory::Zero,
            },
        ];
        invoice.tax_basis_total = 400.0;
        invoice.tax_total = 42.0;
        invoice.grand_total = 442.0;
        invoice.due_payable = 442.0;

        let result = validate_en16931(&invoice);
        assert!(result.is_valid(), "result: {:?}", result);
    }

    #[test]
    fn credit_note_passes() {
        let mut invoice = valid_invoice();
        invoice.type_code = "381".into();
        invoice.invoice_number = "CN-001".into();
        invoice.line_items[0].quantity = -2.0;
        invoice.line_items[0].unit_price = 50.0;
        invoice.line_items[0].line_total = -100.0;
        invoice.tax_basis_total = -100.0;
        invoice.tax_total = -21.0;
        invoice.grand_total = -121.0;
        invoice.due_payable = -121.0;

        let result = validate_en16931(&invoice);
        assert!(result.is_valid(), "result: {:?}", result);
    }

    #[test]
    fn multiple_errors_reported() {
        let mut invoice = valid_invoice();
        invoice.currency = "BAD".into();
        invoice.seller.address.country_code = "ZZ".into();
        invoice.payment_means = None;
        invoice.buyer_reference = None;

        let report = validate_invoice(&invoice);
        assert!(report.error_count() >= 4, "issues: {:?}", report.issues);
        assert!(!report.is_valid());
    }
}
