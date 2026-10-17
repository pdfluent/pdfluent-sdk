//! The 1.1.0 browser tools (#613): password protect and remove (#614), the
//! full text watermark (#615), image extraction (#616) and signing (#617).
//!
//! These call the plain Rust functions behind the exports, so they run under a
//! native `cargo test`. That the exports survive the crossing into JavaScript
//! is checked separately, against the built package, by
//! `tests/browser_tools_node.mjs`.
//!
//! Run: `cargo test -p xfa-wasm --test browser_tools`

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![cfg(not(target_arch = "wasm32"))]

use xfa_wasm::tools::{
    decrypt_bytes, encrypt_bytes, export_image_list, sign_bytes, watermark_bytes,
    EncryptPermissions, SignPdfOptions, VisibleRect, WatermarkOptions, WatermarkPages,
    WatermarkTile,
};

static SIMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");
static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");
static SCANNED_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/scanned.pdf");
static RSA_P12: &[u8] = include_bytes!("../../pdf-sign/tests/fixtures/test-rsa.p12");
static P256_P12: &[u8] = include_bytes!("../../pdf-sign/tests/fixtures/test-ec-p256.p12");

const DENIED: &str = "E-SECURITY-DECRYPTION-FAILED";

fn no_print() -> EncryptPermissions {
    EncryptPermissions {
        print: Some(false),
        print_high_quality: Some(false),
        ..Default::default()
    }
}

/// Encrypted means: lopdf sees an /Encrypt it could not open without a
/// password, and our own parser, which has no password support, refuses it.
fn needs_a_password(bytes: &[u8]) -> bool {
    let doc = lopdf::Document::load_mem(bytes).expect("lopdf loads the encrypted file");
    doc.trailer.has(b"Encrypt") && pdf_syntax::Pdf::new(bytes.to_vec()).is_err()
}

fn is_unprotected(bytes: &[u8]) -> bool {
    let doc = lopdf::Document::load_mem(bytes).expect("decrypted output loads");
    !doc.trailer.has(b"Encrypt") && doc.encryption_state.is_none()
}

// ---------- #614 ------------------------------------------------------------------

#[test]
fn a_user_password_round_trips_and_the_result_opens_again() {
    let enc =
        encrypt_bytes(SIMPLE_PDF, "open-me", "", &EncryptPermissions::default()).expect("encrypt");
    assert!(
        needs_a_password(&enc),
        "the output must actually be protected"
    );

    let wrong = decrypt_bytes(&enc, "nope").unwrap_err();
    assert_eq!(wrong.code, DENIED);

    let plain = decrypt_bytes(&enc, "open-me").expect("decrypt with the right password");
    assert!(is_unprotected(&plain));
    let reopened = xfa_wasm::PdfDoc::open(&plain).expect("PdfDoc.open on the decrypted bytes");
    assert_eq!(reopened.page_count(), 1);
}

/// A file that opens without a password but carries an owner password and
/// restrictions. Lifting those needs the owner password: the empty password
/// the reader opens it with, or a wrong one, must not do it.
#[test]
fn an_owner_only_file_gives_up_its_restrictions_only_to_the_owner_password() {
    let enc = encrypt_bytes(SIMPLE_PDF, "", "the-owner", &no_print()).expect("encrypt");

    assert_eq!(decrypt_bytes(&enc, "").unwrap_err().code, DENIED);
    assert_eq!(decrypt_bytes(&enc, "guess").unwrap_err().code, DENIED);

    let plain = decrypt_bytes(&enc, "the-owner").expect("owner password lifts restrictions");
    assert!(is_unprotected(&plain));
}

#[test]
fn the_user_password_does_not_lift_restrictions_but_the_owner_password_does() {
    let enc = encrypt_bytes(SIMPLE_PDF, "user", "owner", &no_print()).expect("encrypt");
    let refused = decrypt_bytes(&enc, "user").unwrap_err();
    assert_eq!(refused.code, DENIED);
    assert!(
        refused.message.contains("owner password"),
        "{}",
        refused.message
    );

    assert!(is_unprotected(
        &decrypt_bytes(&enc, "owner").expect("owner")
    ));
}

#[test]
fn an_encrypted_file_is_not_encrypted_again_over_its_owners_restrictions() {
    let enc = encrypt_bytes(SIMPLE_PDF, "", "the-owner", &no_print()).expect("encrypt");
    let err = encrypt_bytes(&enc, "mine", "mine", &EncryptPermissions::default()).unwrap_err();
    assert_eq!(err.code, "E-WASM-INVALID-ARGUMENT");
}

#[test]
fn decrypting_a_file_without_a_password_says_so() {
    let err = decrypt_bytes(SIMPLE_PDF, "x").unwrap_err();
    assert_eq!(err.code, "E-WASM-INVALID-ARGUMENT");
    assert!(err.message.contains("not password-protected"));
}

#[test]
fn encrypting_with_no_password_at_all_is_refused() {
    let err = encrypt_bytes(SIMPLE_PDF, "", "", &EncryptPermissions::default()).unwrap_err();
    assert_eq!(err.code, "E-WASM-INVALID-ARGUMENT");
}

/// The same owner-only case, made by a tool that is not us. Our own encryptor
/// and decryptor agreeing proves little if they share a mistake.
#[test]
fn an_owner_only_file_written_by_qpdf_needs_its_owner_password() {
    let Some(qpdf) = [
        "/usr/local/bin/qpdf",
        "/opt/homebrew/bin/qpdf",
        "/usr/bin/qpdf",
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists()) else {
        eprintln!("SKIPPED (not a pass): no qpdf on this machine to write the fixture");
        return;
    };
    let dir = std::env::temp_dir();
    let src = dir.join(format!("browser-tools-src-{}.pdf", std::process::id()));
    let out = dir.join(format!(
        "browser-tools-owner-only-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&src, SIMPLE_PDF).expect("write source");
    let status = std::process::Command::new(qpdf)
        .args([
            "--encrypt",
            "",
            "qpdf-owner",
            "256",
            "--print=none",
            "--modify=none",
            "--",
        ])
        .arg(&src)
        .arg(&out)
        .status()
        .expect("run qpdf");
    let enc = std::fs::read(&out).expect("qpdf output");
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);
    assert!(status.success(), "qpdf could not encrypt the fixture");

    assert_eq!(decrypt_bytes(&enc, "").unwrap_err().code, DENIED);
    assert!(is_unprotected(
        &decrypt_bytes(&enc, "qpdf-owner").expect("owner password")
    ));
}

// ---------- #615 ------------------------------------------------------------------

/// Pages (1-based) whose content streams draw `needle`.
fn pages_drawing(bytes: &[u8], needle: &str) -> Vec<u32> {
    let doc = lopdf::Document::load_mem(bytes).expect("load watermarked");
    doc.get_pages()
        .into_iter()
        .filter(|(_, id)| {
            let content = doc.get_page_content(*id);
            String::from_utf8_lossy(&content).contains(needle)
        })
        .map(|(n, _)| n)
        .collect()
}

#[test]
fn a_preset_watermark_lands_only_on_the_selected_pages() {
    let opts = WatermarkOptions {
        preset: Some("confidential".into()),
        pages: Some(WatermarkPages::List(vec![0, 2])),
        ..Default::default()
    };
    let out = watermark_bytes(MULTI_PDF, &opts).expect("watermark");
    assert_eq!(pages_drawing(&out, "(CONFIDENTIAL)"), vec![1, 3]);
}

#[test]
fn a_tiled_watermark_is_drawn_more_than_once_per_page() {
    let opts = WatermarkOptions {
        text: Some("TILE-ME".into()),
        tile: Some(WatermarkTile::On(true)),
        ..Default::default()
    };
    let out = watermark_bytes(SIMPLE_PDF, &opts).expect("watermark");
    let doc = lopdf::Document::load_mem(&out).unwrap();
    let (_, page) = doc.get_pages().into_iter().next().unwrap();
    let content = String::from_utf8_lossy(&doc.get_page_content(page)).into_owned();
    assert!(
        content.matches("(TILE-ME)").count() > 1,
        "tiling drew it once"
    );
}

#[test]
fn the_watermark_colour_is_the_one_asked_for() {
    let opts = WatermarkOptions {
        text: Some("RED".into()),
        color: Some("#ff0000".into()),
        ..Default::default()
    };
    let out = watermark_bytes(SIMPLE_PDF, &opts).expect("watermark");
    let doc = lopdf::Document::load_mem(&out).unwrap();
    let (_, page) = doc.get_pages().into_iter().next().unwrap();
    let content = String::from_utf8_lossy(&doc.get_page_content(page)).into_owned();
    assert!(
        content.contains("1.0 0.0 0.0 rg"),
        "no red fill in:\n{content}"
    );
}

#[test]
fn bad_watermark_options_are_named() {
    let cases: Vec<(WatermarkOptions, &str)> = vec![
        (WatermarkOptions::default(), "E-WASM-INVALID-ARGUMENT"),
        (
            WatermarkOptions {
                text: Some("x".into()),
                opacity: Some(1.5),
                ..Default::default()
            },
            "E-WASM-INVALID-ARGUMENT",
        ),
        (
            WatermarkOptions {
                preset: Some("secret".into()),
                ..Default::default()
            },
            "E-WASM-INVALID-ARGUMENT",
        ),
        (
            WatermarkOptions {
                text: Some("x".into()),
                pages: Some(WatermarkPages::List(vec![7])),
                ..Default::default()
            },
            "E-WASM-PAGE-OUT-OF-RANGE",
        ),
        (
            WatermarkOptions {
                text: Some("x".into()),
                tile: Some(WatermarkTile::Spacing {
                    h_spacing: Some(0.0),
                    v_spacing: None,
                }),
                ..Default::default()
            },
            "E-WASM-INVALID-ARGUMENT",
        ),
    ];
    for (opts, code) in cases {
        let err = watermark_bytes(SIMPLE_PDF, &opts).unwrap_err();
        assert_eq!(err.code, code, "{opts:?}: {}", err.message);
    }
}

#[test]
fn watermark_options_arrive_from_json_with_camel_case_names() {
    let opts: WatermarkOptions = serde_json::from_str(
        r##"{"text":"X","fontSize":30,"position":{"anchor":"bottomLeft","x":10,"y":10},
            "tile":{"hSpacing":150},"pages":{"from":0,"to":1},"layer":"background","color":"#00ff00"}"##,
    )
    .expect("camelCase options parse");
    assert_eq!(
        pages_drawing(&watermark_bytes(MULTI_PDF, &opts).unwrap(), "(X)"),
        vec![1, 2]
    );
    assert!(serde_json::from_str::<WatermarkOptions>(r#"{"txt":"X"}"#).is_err());
}

// ---------- #616 ------------------------------------------------------------------

#[test]
fn a_plain_rgb_image_comes_out_as_a_png_of_its_size() {
    let export = export_image_list(SCANNED_PDF).expect("extract");
    assert!(export.skipped.is_empty(), "{:?}", export.skipped);
    assert_eq!(export.images.len(), 1);
    let img = &export.images[0];
    assert_eq!((img.page, img.width, img.height), (1, 10, 10));
    assert!(img.image.data.starts_with(b"\x89PNG"));
}

// ---------- #617 ------------------------------------------------------------------

fn validates(signed: &[u8]) -> bool {
    let pdf = pdf_syntax::Pdf::new(signed.to_vec()).expect("signed output parses");
    let results = pdf_sign::validate_signatures(&pdf);
    results.len() == 1 && matches!(results[0].status, pdf_sign::ValidationStatus::Valid)
}

#[test]
fn rsa_and_p256_identities_both_make_a_valid_signature() {
    for (name, p12) in [("rsa", RSA_P12), ("p256", P256_P12)] {
        let opts = SignPdfOptions {
            reason: Some("Approved".into()),
            location: Some("Amsterdam".into()),
            ..Default::default()
        };
        let signed = sign_bytes(SIMPLE_PDF, p12, "test123", &opts).expect(name);
        assert!(
            signed.starts_with(SIMPLE_PDF),
            "{name}: not an incremental update"
        );
        assert!(validates(&signed), "{name}: signature does not validate");
    }
}

#[test]
fn a_visible_signature_validates_and_a_bad_rectangle_is_refused() {
    let visible = SignPdfOptions {
        visible_rect: Some(VisibleRect {
            page: 0,
            x: 72.0,
            y: 72.0,
            width: 180.0,
            height: 50.0,
        }),
        ..Default::default()
    };
    let signed = sign_bytes(SIMPLE_PDF, RSA_P12, "test123", &visible).expect("visible");
    assert!(validates(&signed));

    let off_the_end = SignPdfOptions {
        visible_rect: Some(VisibleRect {
            page: 3,
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        }),
        ..Default::default()
    };
    let err = sign_bytes(SIMPLE_PDF, RSA_P12, "test123", &off_the_end).unwrap_err();
    assert_eq!(err.code, "E-WASM-PAGE-OUT-OF-RANGE");
}

#[test]
fn a_wrong_certificate_password_is_a_password_error() {
    let err = sign_bytes(SIMPLE_PDF, RSA_P12, "wrong", &SignPdfOptions::default()).unwrap_err();
    assert_eq!(err.code, DENIED);
    let garbage = sign_bytes(SIMPLE_PDF, b"not a p12", "x", &SignPdfOptions::default());
    assert_eq!(garbage.unwrap_err().code, "E-WASM-INVALID-ARGUMENT");
}
