//! `sign`: PAdES signatures, thin over
//! [`sign`](pdfluent::PdfDocument::sign) with a
//! [`Pkcs12Signer`](pdfluent::Pkcs12Signer).
//!
//! Only basic (B-B) signatures: timestamped profiles need a live TSA, and the
//! TSA request exchange is not exposed by the facade — `--tsa` and
//! `--profile t` report that gap instead of re-implementing RFC 3161.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx};
use crate::password;

/// PAdES profile. Only `basic` signs: `t` needs a live TSA the facade does
/// not expose (see `--tsa`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Profile {
    /// PAdES B-B: basic signature, no timestamp.
    Basic,
    /// PAdES B-T: refused — needs `--tsa`, which the facade cannot serve.
    T,
}

#[derive(Debug, Args)]
pub struct SignArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// PKCS#12 identity (`.p12`/`.pfx`).
    #[arg(long, value_name = "FILE", required = true)]
    pub p12: PathBuf,

    /// Password file for the identity (line 1). Without it,
    /// PDFLUENT_PASSWORD, then an interactive prompt.
    #[arg(long, value_name = "FILE")]
    pub password_file: Option<PathBuf>,

    /// Timestamp authority URL. Accepted and reported: fetching a token
    /// needs a TSA exchange the facade does not expose.
    #[arg(long, value_name = "URL")]
    pub tsa: Option<String>,

    /// PAdES profile (only `basic` signs today).
    #[arg(long, value_enum, default_value = "basic")]
    pub profile: Profile,

    /// Human-readable reason for signing.
    #[arg(long)]
    pub reason: Option<String>,

    /// Location of signing.
    #[arg(long)]
    pub location: Option<String>,

    /// Signer contact information.
    #[arg(long)]
    pub contact_info: Option<String>,

    /// Signature form field name (default `Signature1`).
    #[arg(long)]
    pub field: Option<String>,

    /// Visible signature rectangle: `PAGE,X0,Y0,X1,Y1` in points.
    #[arg(long, value_name = "PAGE,X0,Y0,X1,Y1")]
    pub visible: Option<String>,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

const TSA_GAP: &str = "timestamped signing needs a TSA exchange the `pdfluent` facade does not expose (facade gap, reported rather than re-implemented); only PAdES B-B (--profile basic, no --tsa) is supported today";

pub(crate) fn run(ctx: &Ctx, args: &SignArgs) -> u8 {
    if args.tsa.is_some() {
        return output::error("sign", ctx, "TSA_NOT_SUPPORTED", TSA_GAP);
    }
    if args.profile != Profile::Basic {
        return output::error("sign", ctx, "TSA_NOT_SUPPORTED", TSA_GAP);
    }
    let visible = match args.visible.as_deref() {
        Some(spec) => match parse_visible(spec) {
            Ok(visible) => Some(visible),
            Err(msg) => return output::error("sign", ctx, "BAD_USAGE", &msg),
        },
        None => None,
    };
    let loaded = match input::load("sign", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let mut doc = match input::open("sign", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let pw = match password::required_password(
        ctx,
        "sign",
        args.password_file.as_deref(),
        "PDFLUENT_PASSWORD",
        "p12 password",
    ) {
        Ok(pw) => pw,
        Err(code) => return code,
    };
    let signer = match pdfluent::Pkcs12Signer::from_pfx_file(&args.p12, &pw) {
        Ok(signer) => signer,
        Err(pdfluent::Error::FileNotFound { .. }) => {
            return output::error(
                "sign",
                ctx,
                "FILE_NOT_FOUND",
                &format!("no such identity file: {}", args.p12.display()),
            );
        }
        Err(_) => {
            return output::error(
                "sign",
                ctx,
                "INVALID_IDENTITY",
                "could not load the PKCS#12 identity (wrong password or corrupt file)",
            );
        }
    };
    let mut opts = pdfluent::SignOptions::new();
    if let Some(reason) = args.reason.as_deref() {
        opts = opts.reason(reason);
    }
    if let Some(location) = args.location.as_deref() {
        opts = opts.location(location);
    }
    if let Some(contact) = args.contact_info.as_deref() {
        opts = opts.contact_info(contact);
    }
    if let Some(field) = args.field.as_deref() {
        opts = opts.field_name(field);
    }
    if let Some((page, rect)) = visible {
        if page == 0 || page > doc.page_count() {
            return output::error(
                "sign",
                ctx,
                "PAGE_RANGE",
                &format!(
                    "visible page {page} is outside this document (1–{})",
                    doc.page_count()
                ),
            );
        }
        opts = opts.visible_rect(page, rect);
    }
    if let Err(e) = doc.sign(&signer, opts) {
        return output::facade_error("sign", ctx, &e);
    }
    let saved = match input::save(
        "sign",
        ctx,
        &doc,
        &args.out,
        loaded.path.as_slice(),
        args.force,
    ) {
        Ok(saved) => saved,
        Err(code) => return code,
    };
    let path = saved.path;
    output::success(
        "sign",
        ctx,
        json!({
            "input": loaded.display,
            "profile": "basic",
            "output_path": path.as_ref().map(|p| p.display().to_string()),
        }),
        vec![],
        |ctx| {
            if let Some(path) = path {
                output::note(ctx, &format!("Wrote {}", path.display()));
            }
        },
    )
}

fn parse_visible(spec: &str) -> Result<(usize, [f64; 4]), String> {
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() != 5 {
        return Err(format!("`{spec}` is not PAGE,X0,Y0,X1,Y1"));
    }
    let page: usize = parts[0]
        .trim()
        .parse()
        .map_err(|_| format!("`{spec}` is not PAGE,X0,Y0,X1,Y1"))?;
    let mut rect = [0.0; 4];
    for (i, part) in parts[1..].iter().enumerate() {
        let num: f64 = part
            .trim()
            .parse()
            .map_err(|_| format!("`{spec}` is not PAGE,X0,Y0,X1,Y1"))?;
        if !num.is_finite() {
            return Err(format!("`{spec}` is not PAGE,X0,Y0,X1,Y1"));
        }
        rect[i] = num;
    }
    Ok((page, rect))
}
