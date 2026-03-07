//! EN 16931 business rule validation for ZUGFeRD/Factur-X invoices.
//!
//! Implements a subset of the EN 16931 business rules that can be checked
//! structurally from the invoice data model.

use crate::iso_codes::{is_valid_country, is_valid_currency};
use crate::zugferd::{TaxCategory, ZugferdInvoice};

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

/// Result of validating an invoice against EN 16931.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// All issues found during validation.
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Whether the invoice passes all checks (no errors).
    pub fn is_valid(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    /// Number of errors.
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    /// Number of warnings.
    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }
}

/// Validate an invoice against EN 16931 business rules.
///
/// Returns a [`ValidationReport`] containing all issues found.
pub fn validate_invoice(invoice: &ZugferdInvoice) -> ValidationReport {
    let mut issues = Vec::new();

    check_br_cl_01(invoice, &mut issues);
    check_br_cl_04(invoice, &mut issues);
    check_br_co_10(invoice, &mut issues);
    check_br_co_11(invoice, &mut issues);
    check_br_co_13(invoice, &mut issues);
    check_br_co_15(invoice, &mut issues);
    check_br_s_08(invoice, &mut issues);

    ValidationReport { issues }
}

/// BR-CL-01: Currency code must be a valid ISO 4217 code.
fn check_br_cl_01(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    if !is_valid_currency(&inv.currency) {
        issues.push(ValidationIssue {
            rule: "BR-CL-01".into(),
            message: format!(
                "Invoice currency code '{}' is not a valid ISO 4217 code",
                inv.currency
            ),
            severity: Severity::Error,
        });
    }
}

/// BR-CL-04: Country codes must be valid ISO 3166-1 alpha-2 codes.
fn check_br_cl_04(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    if !inv.seller.address.country_code.is_empty()
        && !is_valid_country(&inv.seller.address.country_code)
    {
        issues.push(ValidationIssue {
            rule: "BR-CL-04".into(),
            message: format!(
                "Seller country code '{}' is not a valid ISO 3166-1 alpha-2 code",
                inv.seller.address.country_code
            ),
            severity: Severity::Error,
        });
    }
    if !inv.buyer.address.country_code.is_empty()
        && !is_valid_country(&inv.buyer.address.country_code)
    {
        issues.push(ValidationIssue {
            rule: "BR-CL-04".into(),
            message: format!(
                "Buyer country code '{}' is not a valid ISO 3166-1 alpha-2 code",
                inv.buyer.address.country_code
            ),
            severity: Severity::Error,
        });
    }
}

/// BR-CO-10: Sum of line item net amounts = tax basis total.
fn check_br_co_10(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    if inv.line_items.is_empty() {
        return;
    }
    let line_sum: f64 = inv.line_items.iter().map(|li| li.line_total).sum();
    if (line_sum - inv.tax_basis_total).abs() > 0.01 {
        issues.push(ValidationIssue {
            rule: "BR-CO-10".into(),
            message: format!(
                "Sum of line totals ({:.2}) does not match tax basis total ({:.2})",
                line_sum, inv.tax_basis_total
            ),
            severity: Severity::Error,
        });
    }
}

/// BR-CO-11: Tax basis total + tax total = grand total.
fn check_br_co_11(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    let expected = inv.tax_basis_total + inv.tax_total;
    if (expected - inv.grand_total).abs() > 0.01 {
        issues.push(ValidationIssue {
            rule: "BR-CO-11".into(),
            message: format!(
                "Tax basis ({:.2}) + tax ({:.2}) = {:.2}, but grand total is {:.2}",
                inv.tax_basis_total, inv.tax_total, expected, inv.grand_total
            ),
            severity: Severity::Error,
        });
    }
}

/// BR-CO-13: Each line item total = quantity * unit price.
fn check_br_co_13(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    for item in &inv.line_items {
        let expected = item.quantity * item.unit_price;
        if (expected - item.line_total).abs() > 0.01 {
            issues.push(ValidationIssue {
                rule: "BR-CO-13".into(),
                message: format!(
                    "Line '{}': quantity ({}) * unit price ({:.2}) = {:.2}, but line total is {:.2}",
                    item.id, item.quantity, item.unit_price, expected, item.line_total
                ),
                severity: Severity::Error,
            });
        }
    }
}

/// BR-CO-15: Due payable should not exceed grand total.
fn check_br_co_15(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    if inv.due_payable > inv.grand_total + 0.01 {
        issues.push(ValidationIssue {
            rule: "BR-CO-15".into(),
            message: format!(
                "Due payable ({:.2}) exceeds grand total ({:.2})",
                inv.due_payable, inv.grand_total
            ),
            severity: Severity::Warning,
        });
    }
}

/// BR-S-08: For standard-rated items, tax rate must be > 0.
fn check_br_s_08(inv: &ZugferdInvoice, issues: &mut Vec<ValidationIssue>) {
    for item in &inv.line_items {
        if item.tax_category == TaxCategory::Standard && item.tax_rate <= 0.0 {
            issues.push(ValidationIssue {
                rule: "BR-S-08".into(),
                message: format!(
                    "Line '{}': standard-rated item must have tax rate > 0, got {:.2}%",
                    item.id, item.tax_rate
                ),
                severity: Severity::Error,
            });
        }
    }
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
                    street: None,
                    city: None,
                    postal_code: None,
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
                line_total: 1000.0,
                tax_rate: 21.0,
                tax_category: TaxCategory::Standard,
            }],
            currency: "EUR".into(),
            tax_basis_total: 1000.0,
            tax_total: 210.0,
            grand_total: 1210.0,
            due_payable: 1210.0,
            payment_terms: None,
            buyer_reference: None,
        }
    }

    #[test]
    fn valid_invoice_passes() {
        let report = validate_invoice(&valid_invoice());
        assert!(report.is_valid(), "issues: {:?}", report.issues);
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn br_cl_01_invalid_currency() {
        let mut inv = valid_invoice();
        inv.currency = "XYZ".into();
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-CL-01"));
    }

    #[test]
    fn br_cl_04_invalid_country() {
        let mut inv = valid_invoice();
        inv.seller.address.country_code = "XX".into();
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-CL-04"));
    }

    #[test]
    fn br_co_10_line_sum_mismatch() {
        let mut inv = valid_invoice();
        inv.tax_basis_total = 999.0; // should be 1000.0
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-CO-10"));
    }

    #[test]
    fn br_co_11_grand_total_mismatch() {
        let mut inv = valid_invoice();
        inv.grand_total = 9999.0; // should be 1210.0
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-CO-11"));
    }

    #[test]
    fn br_co_13_line_total_mismatch() {
        let mut inv = valid_invoice();
        inv.line_items[0].line_total = 500.0; // should be 1000.0
        inv.tax_basis_total = 500.0;
        inv.tax_total = 105.0;
        inv.grand_total = 605.0;
        inv.due_payable = 605.0;
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-CO-13"));
    }

    #[test]
    fn br_co_15_due_exceeds_grand() {
        let mut inv = valid_invoice();
        inv.due_payable = 9999.0;
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-CO-15"));
    }

    #[test]
    fn br_s_08_zero_rate_standard() {
        let mut inv = valid_invoice();
        inv.line_items[0].tax_rate = 0.0;
        let report = validate_invoice(&inv);
        assert!(report.issues.iter().any(|i| i.rule == "BR-S-08"));
    }

    #[test]
    fn zero_rated_items_allowed() {
        let mut inv = valid_invoice();
        inv.line_items[0].tax_category = TaxCategory::Zero;
        inv.line_items[0].tax_rate = 0.0;
        inv.tax_total = 0.0;
        inv.grand_total = 1000.0;
        inv.due_payable = 1000.0;
        let report = validate_invoice(&inv);
        assert!(!report.issues.iter().any(|i| i.rule == "BR-S-08"));
    }

    #[test]
    fn multiple_errors_reported() {
        let mut inv = valid_invoice();
        inv.currency = "INVALID".into();
        inv.seller.address.country_code = "ZZ".into();
        inv.grand_total = 0.0;
        let report = validate_invoice(&inv);
        assert!(report.error_count() >= 3);
    }

    #[test]
    fn report_counts() {
        let mut inv = valid_invoice();
        inv.due_payable = 99999.0; // warning
        inv.currency = "BAD".into(); // error
        let report = validate_invoice(&inv);
        assert!(report.error_count() >= 1);
        assert!(report.warning_count() >= 1);
        assert!(!report.is_valid());
    }
}
