//! `protect` / `unprotect`: encryption, thin over
//! [`encrypt`](pdfluent::PdfDocument::encrypt) and
//! [`decrypt`](pdfluent::PdfDocument::decrypt).
//!
//! Passwords arrive via `--password-file`, the environment, or a prompt —
//! never argv. See [`crate::password`].

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

/// Encryption algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Algorithm {
    /// AES-256 (default).
    Aes256,
    /// AES-128, for readers older than PDF 1.7 ext.
    Aes128,
}

/// Reader permissions for the protected document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Permissions {
    /// Authorised readers may do anything.
    Full,
    /// Printing only.
    PrintOnly,
    /// Reading only.
    ReadOnly,
    /// Reading plus annotation.
    Annotate,
}

impl From<Permissions> for pdfluent::Permissions {
    fn from(value: Permissions) -> Self {
        match value {
            Permissions::Full => pdfluent::Permissions::full_access(),
            Permissions::PrintOnly => pdfluent::Permissions::print_only(),
            Permissions::ReadOnly => pdfluent::Permissions::read_only(),
            Permissions::Annotate => pdfluent::Permissions::annotate(),
        }
    }
}

#[derive(Debug, Args)]
pub struct ProtectArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Password file: line 1 is the user password, line 2 (if present)
    /// the owner password. Without it, PDFLUENT_PASSWORD and
    /// PDFLUENT_OWNER_PASSWORD, then an interactive prompt.
    #[arg(long, value_name = "FILE")]
    pub password_file: Option<PathBuf>,

    /// Owner-password file (line 1). Overrides line 2 of --password-file.
    #[arg(long, value_name = "FILE")]
    pub owner_password_file: Option<PathBuf>,

    /// Encryption algorithm.
    #[arg(long, value_enum, default_value = "aes256")]
    pub algorithm: Algorithm,

    /// Reader permissions.
    #[arg(long, value_enum, default_value = "full")]
    pub permissions: Permissions,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

pub(crate) fn run_protect(ctx: &Ctx, args: &ProtectArgs) -> u8 {
    let loaded = match input::load("protect", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let mut doc = match input::open("protect", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let user = match password::required_password(
        ctx,
        "protect",
        args.password_file.as_deref(),
        "PDFLUENT_PASSWORD",
        "user password",
    ) {
        Ok(pw) => pw,
        Err(code) => return code,
    };
    // Owner password: the explicit flags win over the environment —
    // --owner-password-file, then line 2 of --password-file, then
    // PDFLUENT_OWNER_PASSWORD, then — on a TTY with no file at all — prompt.
    // Absent means "same as user" (the facade clones the user password).
    let owner: Option<String> = if let Some(path) = args.owner_password_file.as_deref() {
        let pw = match password::read_password_file(path, 1) {
            Ok(pw) => pw,
            Err((code, msg)) => return output::error("protect", ctx, &code, &msg),
        };
        (!pw.is_empty()).then_some(pw)
    } else if let Some(pf) = args.password_file.as_deref() {
        let line2 = password::read_password_file(pf, 2).unwrap_or_default();
        if line2.is_empty() {
            std::env::var("PDFLUENT_OWNER_PASSWORD")
                .ok()
                .filter(|pw| !pw.is_empty())
        } else {
            Some(line2)
        }
    } else if let Ok(pw) = std::env::var("PDFLUENT_OWNER_PASSWORD") {
        (!pw.is_empty()).then_some(pw)
    } else {
        match password::optional_password(
            ctx,
            "protect",
            None,
            "PDFLUENT_OWNER_PASSWORD",
            "owner password (empty = same as user)",
        ) {
            Ok(owner) => owner,
            Err(code) => return code,
        }
    };
    let mut opts = match args.algorithm {
        Algorithm::Aes256 => pdfluent::EncryptOptions::aes256(),
        Algorithm::Aes128 => pdfluent::EncryptOptions::aes128(),
    }
    .with_user_password(user)
    .with_permissions(args.permissions.into());
    if let Some(owner) = owner {
        opts = opts.with_owner_password(owner);
    }
    if let Err(e) = doc.encrypt(opts) {
        return output::facade_error("protect", ctx, &e);
    }
    let saved = match input::save(
        "protect",
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
        "protect",
        ctx,
        json!({
            "input": loaded.display,
            "algorithm": match args.algorithm {
                Algorithm::Aes256 => "aes256",
                Algorithm::Aes128 => "aes128",
            },
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

#[derive(Debug, Args)]
pub struct UnprotectArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Password file (line 1 is the current password). Without it,
    /// PDFLUENT_PASSWORD, then an interactive prompt.
    #[arg(long, value_name = "FILE")]
    pub password_file: Option<PathBuf>,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

pub(crate) fn run_unprotect(ctx: &Ctx, args: &UnprotectArgs) -> u8 {
    let loaded = match input::load("unprotect", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    // Open without a password first: unencrypted input needs none, and a
    // wrong password must not be blamed on a file that never asked for one.
    let doc = match pdfluent::PdfDocument::from_bytes(&loaded.bytes) {
        Ok(mut doc) => {
            // `decrypt` on input without any encryption is a no-op, so
            // `unprotect` is idempotent. A refusal here means owner
            // restrictions behind an empty user password — re-open pristine
            // (a failed decrypt may leave partial state) and ask for the
            // password that lifts them.
            if let Err(e) = doc.decrypt("") {
                if !matches!(e, pdfluent::Error::DecryptionFailed { .. }) {
                    return output::facade_error("unprotect", ctx, &e);
                }
                let pw = match password::required_password(
                    ctx,
                    "unprotect",
                    args.password_file.as_deref(),
                    "PDFLUENT_PASSWORD",
                    "current password",
                ) {
                    Ok(pw) => pw,
                    Err(code) => return code,
                };
                let mut fresh = match pdfluent::PdfDocument::from_bytes(&loaded.bytes) {
                    Ok(doc) => doc,
                    Err(e) => return output::facade_error("unprotect", ctx, &e),
                };
                if let Err(e) = fresh.decrypt(&pw) {
                    return output::facade_error("unprotect", ctx, &e);
                }
                fresh
            } else {
                doc
            }
        }
        Err(pdfluent::Error::DecryptionFailed { .. }) => {
            let pw = match password::required_password(
                ctx,
                "unprotect",
                args.password_file.as_deref(),
                "PDFLUENT_PASSWORD",
                "current password",
            ) {
                Ok(pw) => pw,
                Err(code) => return code,
            };
            match pdfluent::PdfDocument::from_bytes_with(
                &loaded.bytes,
                pdfluent::OpenOptions::new().with_password(&pw),
            ) {
                Err(e) => return output::facade_error("unprotect", ctx, &e),
                // The password is right, but removing it is a facade gap:
                // the facade cannot serialise a password-opened document
                // back to valid unencrypted bytes (its `decrypt` re-decrypts
                // in-memory state and its save path emits unopenable bytes).
                // Report it — writing those bytes would corrupt the file.
                Ok(_) => {
                    return output::error(
                        "unprotect",
                        ctx,
                        "DECRYPT_NOT_SUPPORTED",
                        "removing a user password needs an encrypted round-trip the `pdfluent` facade does not support (facade gap, reported rather than re-implemented); files that open without a password (owner restrictions only) are supported",
                    );
                }
            }
        }
        Err(e) => return output::facade_error("unprotect", ctx, &e),
    };
    let page_count = doc.page_count();
    let saved = match input::save(
        "unprotect",
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
        "unprotect",
        ctx,
        json!({
            "input": loaded.display,
            "page_count": page_count,
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
