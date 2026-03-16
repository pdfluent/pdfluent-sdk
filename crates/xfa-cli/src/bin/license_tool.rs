//! License key generation and validation CLI tool.
//!
//! Generate Ed25519-signed license files and validate existing ones.
//!
//! # Usage
//!
//! ```sh
//! # Generate an Ed25519 keypair
//! xfa-license-tool keygen --output xfa-license
//!
//! # Generate a license file
//! xfa-license-tool generate --customer acme-corp --tier professional \
//!     --days 365 --private-key xfa-license.private
//!
//! # Validate a license file
//! xfa-license-tool validate --license acme.json --public-key xfa-license.public
//!
//! # Inspect a license file (no signature verification)
//! xfa-license-tool inspect --license acme.json
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::time::{SystemTime, UNIX_EPOCH};
use xfa_license::{token, LicenseGuard, LicensePayload, Tier};

#[derive(Parser)]
#[command(name = "xfa-license-tool", about = "XFA license key management")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new Ed25519 keypair and write it to two files.
    Keygen {
        /// Output filename prefix. Writes <prefix>.private and <prefix>.public.
        #[arg(long, default_value = "xfa-license")]
        output: String,
    },
    /// Generate a signed license file.
    Generate {
        /// Licensee / customer name.
        #[arg(long)]
        customer: String,
        /// License tier.
        #[arg(long, value_parser = parse_tier)]
        tier: Tier,
        /// License validity in days.
        #[arg(long, default_value = "365")]
        days: u64,
        /// Path to the 32-byte Ed25519 private key file.
        #[arg(long)]
        private_key: String,
        /// Contact email address (optional).
        #[arg(long, default_value = "")]
        email: String,
        /// Company name (optional).
        #[arg(long, default_value = "")]
        company: String,
        /// Number of seats.
        #[arg(long, default_value = "1")]
        seats: u32,
    },
    /// Validate a license file against a public key.
    Validate {
        /// Path to the license JSON file.
        #[arg(long)]
        license: String,
        /// Path to the 32-byte Ed25519 public key file.
        #[arg(long)]
        public_key: String,
    },
    /// Inspect a license file payload without verifying the signature.
    Inspect {
        /// Path to the license JSON file.
        #[arg(long)]
        license: String,
    },
}

fn parse_tier(s: &str) -> std::result::Result<Tier, String> {
    match s.to_lowercase().as_str() {
        "trial" => Ok(Tier::Trial),
        "basic" => Ok(Tier::Basic),
        "professional" | "pro" => Ok(Tier::Professional),
        "enterprise" | "ent" => Ok(Tier::Enterprise),
        "archival" => Ok(Tier::Archival),
        _ => Err(format!(
            "unknown tier: {s} (use trial/basic/professional/enterprise/archival)"
        )),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}

/// Read a 32-byte Ed25519 key from a file.
fn read_key_file(path: &str) -> Result<[u8; 32]> {
    let bytes = std::fs::read(path).with_context(|| format!("failed to read key file: {path}"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("key file {path} must be exactly 32 bytes"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Keygen { output } => {
            let (private_key, public_key) = token::generate_keypair();
            let private_path = format!("{output}.private");
            let public_path = format!("{output}.public");
            std::fs::write(&private_path, private_key)
                .with_context(|| format!("failed to write {private_path}"))?;
            std::fs::write(&public_path, public_key)
                .with_context(|| format!("failed to write {public_path}"))?;
            println!("Keypair generated:");
            println!("  Private key: {private_path}");
            println!("  Public key:  {public_path}");
        }

        Command::Generate {
            customer,
            tier,
            days,
            private_key,
            email,
            company,
            seats,
        } => {
            let private_key_bytes = read_key_file(&private_key)?;
            let issued = now_unix();
            let secs_per_day: u64 = 86_400;
            let expires = issued
                .checked_add(days.checked_mul(secs_per_day).context("days overflow")?)
                .context("expiry overflow")?;

            let payload = LicensePayload {
                licensee: customer.clone(),
                email,
                company,
                tier,
                seats,
                issued_at: issued,
                expires_at: expires,
                features: None,
            };

            let license_json = token::sign_license(&private_key_bytes, &payload)
                .context("failed to sign license")?;

            println!("License generated:");
            println!("  Licensee:  {customer}");
            println!("  Tier:      {tier:?}");
            println!("  Issued:    {issued}");
            println!("  Expires:   {expires}");
            println!();
            println!("{license_json}");
        }

        Command::Validate {
            license,
            public_key,
        } => {
            let public_key_bytes = read_key_file(&public_key)?;
            let license_json = std::fs::read_to_string(&license)
                .with_context(|| format!("failed to read license file: {license}"))?;
            let now = now_unix();

            // Fixes: from_token → from_license (Ed25519 public-key verification)
            match LicenseGuard::from_license(&public_key_bytes, &license_json, now) {
                Ok(guard) => {
                    println!("Valid license:");
                    // Fixes: customer_id() → licensee()
                    println!("  Licensee:    {}", guard.licensee());
                    println!("  Company:     {}", guard.company());
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
                    println!(
                        "  Page quota:  {} pages/period",
                        claims.quotas.pages_rendered
                    );
                }
                Err(e) => {
                    eprintln!("License validation failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        Command::Inspect { license } => {
            let license_json = std::fs::read_to_string(&license)
                .with_context(|| format!("failed to read license file: {license}"))?;
            // Parse the payload without verifying the signature.
            let license_file: xfa_license::LicenseFile =
                serde_json::from_str(&license_json).context("failed to parse license JSON")?;
            println!("{}", serde_json::to_string_pretty(&license_file)?);
        }
    }

    Ok(())
}
