// Copyright (c) 2026 PDFluent B.V. All rights reserved.

//! xfa-license-gen — CLI tool for Ed25519 license key generation, signing, and verification.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use xfa_license::claims::{LicensePayload, Tier};
use xfa_license::token;

#[derive(Parser)]
#[command(
    name = "xfa-license-gen",
    about = "Generate and verify Ed25519-signed XFA license files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new Ed25519 keypair (private.key and public.key).
    Keygen,

    /// Sign a license payload and write a license JSON file.
    Sign {
        /// Path to the private key file (32-byte hex).
        #[arg(long)]
        private_key: PathBuf,

        /// Licensee name.
        #[arg(long)]
        licensee: String,

        /// Contact email.
        #[arg(long)]
        email: String,

        /// Company name.
        #[arg(long)]
        company: String,

        /// License tier.
        #[arg(long, value_parser = parse_tier)]
        tier: Tier,

        /// Number of seats.
        #[arg(long)]
        seats: u32,

        /// Expiry date (YYYY-MM-DD).
        #[arg(long)]
        expires: String,

        /// Output path for the license JSON file.
        #[arg(long)]
        output: PathBuf,
    },

    /// Verify a signed license file and print its contents.
    Verify {
        /// Path to the public key file (32-byte hex).
        #[arg(long)]
        public_key: PathBuf,

        /// Path to the license JSON file.
        #[arg(long)]
        license: PathBuf,
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
            "unknown tier: {s} (valid: trial, basic, professional, enterprise, archival)"
        )),
    }
}

/// Parse a YYYY-MM-DD date string into a Unix timestamp (end of day UTC).
fn parse_date_to_unix(date: &str) -> Result<u64> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        anyhow::bail!("invalid date format: expected YYYY-MM-DD, got {date}");
    }
    let year: i64 = parts[0]
        .parse()
        .with_context(|| format!("invalid year: {}", parts[0]))?;
    let month: i64 = parts[1]
        .parse()
        .with_context(|| format!("invalid month: {}", parts[1]))?;
    let day: i64 = parts[2]
        .parse()
        .with_context(|| format!("invalid day: {}", parts[2]))?;

    if !(1..=12).contains(&month) {
        anyhow::bail!("month out of range: {month}");
    }
    if !(1..=31).contains(&day) {
        anyhow::bail!("day out of range: {day}");
    }

    // Civil date to Unix timestamp using Howard Hinnant's algorithm.
    let days = days_from_civil(year, month, day);
    let epoch_days = days_from_civil(1970, 1, 1);
    let unix_days = days - epoch_days;

    // End of day: 23:59:59
    let secs = unix_days * 86400 + 86399;
    if secs < 0 {
        anyhow::bail!("date {date} is before Unix epoch");
    }
    Ok(secs as u64)
}

/// Civil date to day count (algorithm from Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = month as u64;
    let doy = if m > 2 {
        (153 * (m - 3) + 2) / 5 + day as u64 - 1
    } else {
        (153 * (m + 9) + 2) / 5 + day as u64 - 1
    };
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}

/// Read a hex-encoded key file and decode to raw bytes.
fn read_hex_key(path: &std::path::Path) -> Result<Vec<u8>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read key file: {}", path.display()))?;
    hex::decode(content.trim())
        .with_context(|| format!("failed to decode hex from: {}", path.display()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Keygen => {
            let (private_key, public_key) = token::generate_keypair();

            let private_hex = hex::encode(private_key);
            let public_hex = hex::encode(public_key);

            std::fs::write("private.key", &private_hex).context("failed to write private.key")?;
            std::fs::write("public.key", &public_hex).context("failed to write public.key")?;

            println!("Keypair generated:");
            println!("  private.key  ({} bytes)", private_key.len());
            println!("  public.key   ({} bytes)", public_key.len());
        }

        Command::Sign {
            private_key,
            licensee,
            email,
            company,
            tier,
            seats,
            expires,
            output,
        } => {
            let key_bytes = read_hex_key(&private_key)?;
            let expires_at = parse_date_to_unix(&expires)?;
            let issued_at = now_unix();

            let payload = LicensePayload {
                licensee,
                email,
                company,
                tier,
                seats,
                issued_at,
                expires_at,
                features: None,
            };

            let license_json =
                token::sign_license(&key_bytes, &payload).context("failed to sign license")?;

            std::fs::write(&output, &license_json)
                .with_context(|| format!("failed to write license to {}", output.display()))?;

            println!("License signed and written to {}", output.display());
            println!("  Licensee:  {}", payload.licensee);
            println!("  Email:     {}", payload.email);
            println!("  Company:   {}", payload.company);
            println!("  Tier:      {tier:?}");
            println!("  Seats:     {seats}");
            println!("  Issued:    {issued_at}");
            println!("  Expires:   {expires} ({expires_at})");
        }

        Command::Verify {
            public_key,
            license,
        } => {
            let key_bytes = read_hex_key(&public_key)?;
            let license_json = std::fs::read_to_string(&license)
                .with_context(|| format!("failed to read license: {}", license.display()))?;

            match token::verify_license(&key_bytes, &license_json) {
                Ok(license_file) => {
                    let p = &license_file.payload;
                    println!("License is valid.");
                    println!("  Licensee:  {}", p.licensee);
                    println!("  Email:     {}", p.email);
                    println!("  Company:   {}", p.company);
                    println!("  Tier:      {:?}", p.tier);
                    println!("  Seats:     {}", p.seats);
                    println!("  Issued:    {}", p.issued_at);
                    println!("  Expires:   {}", p.expires_at);

                    if p.is_expired(now_unix()) {
                        println!("  Status:    EXPIRED");
                    } else {
                        println!("  Status:    ACTIVE");
                    }

                    if let Some(ref features) = p.features {
                        println!("  Features:  {}", features.join(", "));
                    }
                }
                Err(e) => {
                    eprintln!("License verification failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_valid() {
        let ts = parse_date_to_unix("2026-12-31").unwrap();
        // 2026-12-31 23:59:59 UTC = 1798761599
        assert_eq!(ts, 1798761599);
    }

    #[test]
    fn parse_date_invalid_format() {
        assert!(parse_date_to_unix("2026/12/31").is_err());
        assert!(parse_date_to_unix("not-a-date").is_err());
    }

    #[test]
    fn parse_tier_valid() {
        assert_eq!(parse_tier("trial").unwrap(), Tier::Trial);
        assert_eq!(parse_tier("basic").unwrap(), Tier::Basic);
        assert_eq!(parse_tier("professional").unwrap(), Tier::Professional);
        assert_eq!(parse_tier("pro").unwrap(), Tier::Professional);
        assert_eq!(parse_tier("enterprise").unwrap(), Tier::Enterprise);
        assert_eq!(parse_tier("ent").unwrap(), Tier::Enterprise);
        assert_eq!(parse_tier("archival").unwrap(), Tier::Archival);
    }

    #[test]
    fn parse_tier_invalid() {
        assert!(parse_tier("unknown").is_err());
    }
}
