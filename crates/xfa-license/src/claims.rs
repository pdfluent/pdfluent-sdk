//! License claims — tier definitions, feature flags, and quotas.

use serde::{Deserialize, Serialize};

/// License tier levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Free evaluation tier — watermarked output, limited API calls.
    Trial,
    /// Basic tier — core XFA parsing and rendering.
    Basic,
    /// Professional tier — flattening, PDF/A, signatures.
    Professional,
    /// Enterprise tier — all features, high quotas, priority support.
    Enterprise,
    /// Archival tier — long-term compliance features (PDF/A, LTV).
    Archival,
}

impl Tier {
    /// Default rate limit (requests per minute) for each tier.
    pub fn default_rate_limit(&self) -> u32 {
        match self {
            Tier::Trial => 10,
            Tier::Basic => 60,
            Tier::Professional => 300,
            Tier::Enterprise => 1000,
            Tier::Archival => 500,
        }
    }

    /// Default monthly API call quota.
    pub fn default_api_quota(&self) -> u64 {
        match self {
            Tier::Trial => 100,
            Tier::Basic => 10_000,
            Tier::Professional => 100_000,
            Tier::Enterprise => 1_000_000,
            Tier::Archival => 500_000,
        }
    }

    /// Default monthly page rendering quota.
    pub fn default_page_quota(&self) -> u64 {
        match self {
            Tier::Trial => 500,
            Tier::Basic => 50_000,
            Tier::Professional => 500_000,
            Tier::Enterprise => 5_000_000,
            Tier::Archival => 2_000_000,
        }
    }
}

/// Feature flags controlling which engine capabilities are available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureFlags {
    /// XFA parsing and template extraction.
    pub xfa_parse: bool,
    /// Form field extraction (PDF → JSON).
    pub field_extract: bool,
    /// Form filling (JSON → PDF).
    pub form_fill: bool,
    /// Layout engine and rendering.
    pub render: bool,
    /// XFA → AcroForm flattening.
    pub flatten: bool,
    /// PDF/A output compliance.
    pub pdfa: bool,
    /// Digital signature handling (DocMDP, FieldMDP).
    pub signatures: bool,
    /// Color space conversion.
    pub colorspace: bool,
    /// FormCalc scripting execution.
    pub scripting: bool,
    /// REST API access.
    pub api_access: bool,
}

impl FeatureFlags {
    /// Feature set for a given tier.
    pub fn for_tier(tier: Tier) -> Self {
        match tier {
            Tier::Trial => Self {
                xfa_parse: true,
                field_extract: true,
                form_fill: false,
                render: true,
                flatten: false,
                pdfa: false,
                signatures: false,
                colorspace: false,
                scripting: false,
                api_access: false,
            },
            Tier::Basic => Self {
                xfa_parse: true,
                field_extract: true,
                form_fill: true,
                render: true,
                flatten: false,
                pdfa: false,
                signatures: false,
                colorspace: false,
                scripting: true,
                api_access: true,
            },
            Tier::Professional => Self {
                xfa_parse: true,
                field_extract: true,
                form_fill: true,
                render: true,
                flatten: true,
                pdfa: false,
                signatures: true,
                colorspace: true,
                scripting: true,
                api_access: true,
            },
            Tier::Enterprise => Self {
                xfa_parse: true,
                field_extract: true,
                form_fill: true,
                render: true,
                flatten: true,
                pdfa: true,
                signatures: true,
                colorspace: true,
                scripting: true,
                api_access: true,
            },
            Tier::Archival => Self {
                xfa_parse: true,
                field_extract: true,
                form_fill: true,
                render: true,
                flatten: true,
                pdfa: true,
                signatures: true,
                colorspace: true,
                scripting: true,
                api_access: true,
            },
        }
    }

    /// Check whether a named feature is enabled.
    pub fn has_feature(&self, name: &str) -> bool {
        match name {
            "xfa_parse" => self.xfa_parse,
            "field_extract" => self.field_extract,
            "form_fill" => self.form_fill,
            "render" => self.render,
            "flatten" => self.flatten,
            "pdfa" => self.pdfa,
            "signatures" => self.signatures,
            "colorspace" => self.colorspace,
            "scripting" => self.scripting,
            "api_access" => self.api_access,
            _ => false,
        }
    }
}

/// Quota limits per billing period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quotas {
    /// Maximum API calls per billing period (0 = unlimited).
    pub api_calls: u64,
    /// Maximum pages rendered per billing period (0 = unlimited).
    pub pages_rendered: u64,
    /// Maximum forms processed per billing period (0 = unlimited).
    pub forms_processed: u64,
}

impl Quotas {
    /// Default quotas for a given tier.
    pub fn for_tier(tier: Tier) -> Self {
        Self {
            api_calls: tier.default_api_quota(),
            pages_rendered: tier.default_page_quota(),
            forms_processed: 0, // unlimited by default
        }
    }
}

/// License claims embedded in a signed token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseClaims {
    /// Unique customer identifier.
    pub customer_id: String,
    /// License tier.
    pub tier: Tier,
    /// Unix timestamp when the license was issued.
    pub issued_at: u64,
    /// Unix timestamp when the license expires.
    pub expires_at: u64,
    /// Feature flags.
    pub features: FeatureFlags,
    /// Quota limits.
    pub quotas: Quotas,
    /// Rate limit (requests per minute).
    pub rate_limit: u32,
}

impl LicenseClaims {
    /// Create claims for a customer with tier defaults.
    pub fn new(
        customer_id: impl Into<String>,
        tier: Tier,
        issued_at: u64,
        expires_at: u64,
    ) -> Self {
        Self {
            customer_id: customer_id.into(),
            tier,
            issued_at,
            expires_at,
            features: FeatureFlags::for_tier(tier),
            quotas: Quotas::for_tier(tier),
            rate_limit: tier.default_rate_limit(),
        }
    }

    /// Check if the license has expired given the current unix timestamp.
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }
}

/// The unsigned payload of a license file (Ed25519).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicensePayload {
    /// Name of the license holder.
    pub licensee: String,
    /// Contact email.
    pub email: String,
    /// Company name.
    pub company: String,
    /// License tier.
    pub tier: Tier,
    /// Number of seats.
    pub seats: u32,
    /// Unix timestamp when the license was issued.
    pub issued_at: u64,
    /// Unix timestamp when the license expires.
    pub expires_at: u64,
    /// Optional feature overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
}

/// A signed license file loaded from disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseFile {
    /// The license payload.
    #[serde(flatten)]
    pub payload: LicensePayload,
    /// Base64-encoded Ed25519 signature over the canonical payload JSON.
    pub signature: String,
}

impl LicensePayload {
    /// Convert to LicenseClaims for use with the metering system.
    pub fn to_claims(&self) -> LicenseClaims {
        LicenseClaims::new(&self.licensee, self.tier, self.issued_at, self.expires_at)
    }

    /// Check if the license has expired.
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_defaults() {
        assert_eq!(Tier::Trial.default_rate_limit(), 10);
        assert_eq!(Tier::Enterprise.default_api_quota(), 1_000_000);
        assert_eq!(Tier::Professional.default_page_quota(), 500_000);
    }

    #[test]
    fn feature_flags_for_tier() {
        let trial = FeatureFlags::for_tier(Tier::Trial);
        assert!(trial.xfa_parse);
        assert!(!trial.flatten);
        assert!(!trial.api_access);

        let pro = FeatureFlags::for_tier(Tier::Professional);
        assert!(pro.flatten);
        assert!(pro.signatures);
        assert!(!pro.pdfa);

        let ent = FeatureFlags::for_tier(Tier::Enterprise);
        assert!(ent.pdfa);
        assert!(ent.api_access);
    }

    #[test]
    fn has_feature_lookup() {
        let flags = FeatureFlags::for_tier(Tier::Basic);
        assert!(flags.has_feature("xfa_parse"));
        assert!(flags.has_feature("form_fill"));
        assert!(!flags.has_feature("flatten"));
        assert!(!flags.has_feature("unknown_feature"));
    }

    #[test]
    fn claims_new_applies_defaults() {
        let claims = LicenseClaims::new("cust-123", Tier::Professional, 1000, 2000);
        assert_eq!(claims.customer_id, "cust-123");
        assert_eq!(claims.tier, Tier::Professional);
        assert_eq!(claims.rate_limit, 300);
        assert!(claims.features.flatten);
        assert_eq!(claims.quotas.api_calls, 100_000);
    }

    #[test]
    fn expiry_check() {
        let claims = LicenseClaims::new("c", Tier::Trial, 100, 200);
        assert!(!claims.is_expired(150));
        assert!(claims.is_expired(200));
        assert!(claims.is_expired(300));
    }

    #[test]
    fn claims_serde_roundtrip() {
        let claims = LicenseClaims::new("test", Tier::Enterprise, 1000, 2000);
        let json = serde_json::to_string(&claims).unwrap();
        let parsed: LicenseClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims, parsed);
    }
}
