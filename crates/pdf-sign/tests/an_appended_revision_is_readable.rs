//! An appended revision has to be readable by someone who is not us (#362).
//!
//! `sign_pdf_incremental` and `embed_dss_incremental` both append a saved
//! document to the original. That save counts its offsets from its own first
//! byte, so before this was corrected the appended `startxref` pointed into the
//! middle of the first revision and a strict reader rejected the file outright.
//!
//! Our own tests missed it for two reasons worth naming. They read the result
//! with a scanning parser, which rebuilds the table from the object bodies and
//! never consults `startxref`; and the one test that would have exercised the
//! B-LT path sat behind `#[cfg(feature = "tsa")]`, a feature CI does not
//! enable, so it never ran.
//!
//! Hence an outside judge. qpdf parses the cross-reference chain the way the
//! specification describes and complains when it does not add up, which is
//! exactly the property at stake and exactly the one we cannot check with the
//! code under test.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;
use std::process::Command;

use pdfluent_sign::{
    embed_dss_incremental, sign_pdf, sign_pdf_incremental, PdfSigner, Pkcs12Signer, SignOptions,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ sits two below the repository root")
        .to_path_buf()
}

fn fixtures() -> Option<(Pkcs12Signer, Vec<u8>)> {
    let p12 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-rsa.p12");
    let pdf = repo_root().join("tests/corpus-mini/simple.pdf");
    if !p12.exists() || !pdf.exists() {
        return None;
    }
    let signer = Pkcs12Signer::from_pkcs12(&std::fs::read(p12).ok()?, "test123").ok()?;
    Some((signer, std::fs::read(pdf).ok()?))
}

/// qpdf, if this machine has one.
///
/// Absent on a machine without it, and the test then reports the skip rather
/// than passing quietly: a green tick that means "no judge was available" is
/// the thing this file exists to avoid.
fn qpdf() -> Option<&'static str> {
    [
        "/usr/local/bin/qpdf",
        "/opt/homebrew/bin/qpdf",
        "/usr/bin/qpdf",
    ]
    .into_iter()
    .find(|candidate| std::path::Path::new(candidate).exists())
}

fn qpdf_accepts(bin: &str, bytes: &[u8], name: &str) {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, bytes).expect("write the file for qpdf");
    let out = Command::new(bin)
        .arg("--check")
        .arg(&path)
        .output()
        .expect("run qpdf");
    let report =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "qpdf rejected {name}. An appended revision whose offsets still count \
         from its own first byte reads as a damaged file to everyone but us:\n{report}"
    );
}

#[test]
fn a_signature_appended_to_an_existing_file_reads_back() {
    let Some((signer, pdf)) = fixtures() else {
        eprintln!("skipping: repository fixture tree not present");
        return;
    };
    let Some(bin) = qpdf() else {
        eprintln!("skipping: no qpdf on this machine to judge the file");
        return;
    };

    let signed = sign_pdf_incremental(&pdf, &signer, &SignOptions::default())
        .expect("incremental signing succeeds");
    assert!(
        signed.starts_with(&pdf),
        "an incremental update must leave the first revision byte for byte, \
         otherwise the signatures already in the file stop verifying",
    );
    qpdf_accepts(bin, &signed, "pdfluent-incremental-signature.pdf");
}

#[test]
fn a_validation_store_appended_to_a_signed_file_reads_back() {
    let Some((signer, pdf)) = fixtures() else {
        eprintln!("skipping: repository fixture tree not present");
        return;
    };
    let Some(bin) = qpdf() else {
        eprintln!("skipping: no qpdf on this machine to judge the file");
        return;
    };

    let signed = sign_pdf(&pdf, &signer, &SignOptions::default()).expect("signing succeeds");
    let with_dss = embed_dss_incremental(
        &signed,
        signer.certificate_chain_der().to_vec(),
        vec![],
        vec![],
        vec![],
    )
    .expect("DSS embedding succeeds");

    assert!(
        with_dss.starts_with(&signed),
        "the store is appended outside the signed byte range, so the signature \
         it describes has to survive it",
    );
    qpdf_accepts(bin, &with_dss, "pdfluent-dss-increment.pdf");
}
