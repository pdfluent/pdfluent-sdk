//! License key generation and validation CLI tool.
//!
//! Generate signed license tokens and validate existing ones.
//!
//! # Usage
//!
//! ```sh
//! # Generate a license key
//! xfa-license-tool generate --customer acme-corp --tier professional \
//!     --days 365 --secret my-secret-key
//!
//! # Validate a license key
//! xfa-license-tool validate --token <token> --secret my-secret-key
//!
//! # Inspect a token (without signature verification)
//! xfa-license-tool inspect --token <token>
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::time::{SystemTime, UNIX_EPOCH};
use xfa_license::{LicenseClaims, LicenseGuard, Tier, token};

#[derive(Parser)]
#[command(name = "xfa-license-tool", about = "XFA license key management")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a signed license token.
    Generate {
        /// Customer identifier.
        #[arg(long)]
        customer: String,
        /// License tier.
        #[arg(long, value_parser = parse_tier)]
        tier: Tier,
        /// License validity in days.
        #[arg(long, default_value = "365")]
        days: u64,
        /// HMAC signing secret.
        #[arg(long)]
        secret: String,
    },
    /// Validate a license token.
    Validate {
        /// The license token string.
        #[arg(long)]
        token: String,
        /// HMAC signing secret.
        #[arg(long)]
        secret: String,
    },
    /// Inspect a token payload (no signature check).
    Inspect {
        /// The license token string.
        #[arg(long)]
        token: String,
    },
}

fn parse_tier(s: &str) -> std::result::Result<Tier, String> {
    match s.to_lowercase().as_str() {
        "trial" => Ok(Tier::Trial),
        "basic" => Ok(Tier::Basic),
        "professional" | "pro" => Ok(Tier::Professional),
        "enterprise" | "ent" => Ok(Tier::Enterprise),
        "archival" => Ok(Tier::Archival),
        _ => Err(format!("unknown tier: {s} (use trial/basic/professional/enterprise/archival)")),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Generate {
            customer,
            tier,
            days,
            secret,
        } => {
            let issued = now_unix();
            let expires = issued + days * 86400;
            let claims = LicenseClaims::new(&customer, tier, issued, expires);
            let token_str =
                token::sign(&claims, secret.as_bytes()).context("failed to sign token")?;

            println!("License generated:");
            println!("  Customer:  {}", claims.customer_id);
            println!("  Tier:      {:?}", claims.tier);
            println!("  Issued:    {issued}");
            println!("  Expires:   {expires}");
            println!("  Rate:      {} req/min", claims.rate_limit);
            println!("  API quota: {} calls/period", claims.quotas.api_calls);
            println!();
            println!("Token:");
            println!("{token_str}");
        }
        Command::Validate { token: tok, secret } => {
            let now = now_unix();
            match LicenseGuard::from_token(&tok, secret.as_bytes(), now) {
                Ok(guard) => {
                    println!("Valid license:");
                    println!("  Customer:    {}", guard.customer_id());
                    println!("  Tier:        {:?}", guard.tier());
                    println!("  Watermark:   {}", guard.should_watermark());
                    println!("  Features:");
                    let claims = guard.claims();
                    for feat in &[
                        "xfa_parse",
                        "field_extract",
                        "form_fill",
                        "render",
                        "flatten",
                        "pdfa",
                        "signatures",
                        "colorspace",
                        "scripting",
                        "api_access",
                    ] {
                        let enabled = claims.features.has_feature(feat);
                        let mark = if enabled { "+" } else { "-" };
                        println!("    [{mark}] {feat}");
                    }
                    println!("  Rate limit:  {} req/min", claims.rate_limit);
                    println!("  API quota:   {} calls/period", claims.quotas.api_calls);
                    println!("  Page quota:  {} pages/period", claims.quotas.pages_rendered);
                }
                Err(e) => {
                    eprintln!("License validation failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Command::Inspect { token: tok } => {
            let parts: Vec<&str> = tok.split('.').collect();
            if parts.len() != 3 {
                anyhow::bail!("malformed token: expected 3 dot-separated parts");
            }
            let payload = base64_decode(parts[1])
                .context("failed to decode payload")?;
            let claims: LicenseClaims = serde_json::from_slice(&payload)
                .context("failed to parse claims JSON")?;
            println!("{}", serde_json::to_string_pretty(&claims)?);
        }
    }

    Ok(())
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    Ok(URL_SAFE_NO_PAD.decode(input)?)
}
