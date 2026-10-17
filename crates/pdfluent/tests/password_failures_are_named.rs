//! "Wrong password" is not what to tell somebody who gave none (#450).
//!
//! The engine reports one condition for both cases -- the document is
//! encrypted and the password it tried did not open it -- and the facade
//! flattened that to "wrong password". A caller who supplied no password was
//! sent hunting a typo they never made. Found while measuring #390 on
//! `PasswordSample-256bit.pdf`, which we refuse with "wrong password" although
//! no password was offered to be wrong.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::error::DecryptionFailureReason;
use pdfluent::prelude::*;
use pdfluent::{Error, OpenOptions};

const SAMPLE: &str = "tests/fixtures/sample.pdf";
const USER_PASSWORD: &str = "user-pw";

/// The sample document, encrypted by us, as bytes.
fn encrypted_sample(options: EncryptOptions) -> Vec<u8> {
    let mut doc = PdfDocument::open(SAMPLE).expect("open sample");
    doc.encrypt(
        options
            .with_user_password(USER_PASSWORD)
            .with_owner_password("owner-pw"),
    )
    .expect("encrypt");
    doc.to_bytes().expect("serialise encrypted sample")
}

fn reason(err: Error) -> DecryptionFailureReason {
    match err {
        Error::DecryptionFailed { reason } => reason,
        other => panic!("expected a decryption failure, got: {other:?}"),
    }
}

#[test]
fn opening_without_a_password_says_one_is_required() {
    for options in [EncryptOptions::aes128(), EncryptOptions::aes256()] {
        let bytes = encrypted_sample(options);
        let err = PdfDocument::from_bytes_with(&bytes, OpenOptions::new())
            .expect_err("an encrypted document does not open unaided");
        assert_eq!(
            reason(err),
            DecryptionFailureReason::PasswordRequired,
            "no password was supplied, so none of them can be the wrong one"
        );
    }
}

#[test]
fn opening_with_the_wrong_password_still_says_so() {
    for options in [EncryptOptions::aes128(), EncryptOptions::aes256()] {
        let bytes = encrypted_sample(options);
        let err = PdfDocument::from_bytes_with(&bytes, OpenOptions::new().with_password("nope"))
            .expect_err("a wrong password does not open the document");
        assert_eq!(reason(err), DecryptionFailureReason::WrongPassword);
    }
}

#[test]
fn the_two_reasons_read_differently() {
    // The message is the whole point of the distinction: a caller reads it
    // before they read the enum.
    assert_eq!(
        DecryptionFailureReason::PasswordRequired.to_string(),
        "this document requires a password"
    );
    assert_eq!(
        DecryptionFailureReason::WrongPassword.to_string(),
        "wrong password"
    );
}

#[test]
fn the_right_password_opens_what_we_encrypted() {
    // The round trip `security.rs` described as a post-1.0 improvement. It
    // works now, and pinning it is what keeps the reasons above meaningful:
    // a build where nothing opens would satisfy both of them.
    for options in [EncryptOptions::aes128(), EncryptOptions::aes256()] {
        let bytes = encrypted_sample(options);
        let doc =
            PdfDocument::from_bytes_with(&bytes, OpenOptions::new().with_password(USER_PASSWORD))
                .expect("the user password opens the document");
        assert_eq!(doc.page_count(), 1);
    }
}
