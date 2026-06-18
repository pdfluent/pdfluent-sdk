package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Parity-closure tests for the commercial-license surface added to the
 * canonical Java binding:
 *
 * <ul>
 *   <li>{@link PdfluentLicensing#setPublicKey(byte[])} +
 *       {@link PdfluentLicensing#activatePayload(String)} — the signed-payload
 *       license path (mirrors Node {@code setLicensePublicKey} /
 *       {@code setLicensePayload}).</li>
 *   <li>{@link PdfDocument#sign(String, String)},
 *       {@link PdfDocument#signatureCount()},
 *       {@link PdfDocument#isSignatureValid(int)} and
 *       {@link PdfDocument#verifySignatures()} — digital signing and signature
 *       inspection (mirrors Node {@code validateSignatures}).</li>
 * </ul>
 *
 * <p>All paths run against the C ABI {@code libpdf_capi}, loaded via JNA. Set
 * {@code -Djna.library.path=<dir containing libpdf_capi>} when running.
 * The PKCS#12 signing fixture lives in {@code crates/pdf-sign/tests/fixtures}.
 */
class SigningAndPayloadTest {

    private static final Path CORPUS =
        Paths.get(System.getenv().getOrDefault(
            "PDFLUENT_CORPUS_MINI",
            "../../tests/corpus-mini"));

    private static final Path RSA_P12 =
        Paths.get(System.getenv().getOrDefault(
            "PDFLUENT_TEST_P12",
            "../../crates/pdf-sign/tests/fixtures/test-rsa.p12"));

    /** Password for {@code test-rsa.p12} (see pdf-sign signer.rs tests). */
    private static final String RSA_P12_PASSWORD = "test123";

    private static PdfDocument openCorpus(String name) throws Exception {
        byte[] bytes = Files.readAllBytes(CORPUS.resolve(name));
        return PdfDocument.open(bytes);
    }

    // ---------------------------------------------------------------------
    // License: signed-payload surface is bound (no UnsatisfiedLinkError)
    // ---------------------------------------------------------------------

    @Test
    void setPublicKeyNullThrows() {
        assertThrows(NullPointerException.class,
            () -> PdfluentLicensing.setPublicKey(null));
    }

    @Test
    void activatePayloadNullThrows() {
        assertThrows(NullPointerException.class,
            () -> PdfluentLicensing.activatePayload(null));
    }

    /**
     * A public key that is not exactly 32 bytes is rejected by the native
     * layer with a typed {@link PdfluentLicenseException} carrying the
     * canonical {@code E-LICENSE-INVALID} code. This also proves the native
     * symbol {@code pdfluent_license_set_public_key} is bound (a missing
     * symbol would surface as {@link UnsatisfiedLinkError}, which is NOT a
     * {@link PdfluentException}).
     */
    @Test
    void setPublicKeyWrongLengthThrowsTypedCode() {
        PdfluentLicenseException e = assertThrows(PdfluentLicenseException.class,
            () -> PdfluentLicensing.setPublicKey(new byte[16]));
        assertEquals("E-LICENSE-INVALID", e.getCode());
    }

    /**
     * A correctly-sized 32-byte key either succeeds (first injection wins) or,
     * if another test already injected a different key in this process, throws
     * the canonical typed conflict code. Either way the call must NOT raise an
     * {@link UnsatisfiedLinkError}: the symbol is bound.
     */
    @Test
    void setPublicKeyThirtyTwoBytesIsBoundAndTyped() {
        byte[] key = new byte[32];
        for (int i = 0; i < key.length; i++) {
            key[i] = (byte) (i + 1);
        }
        try {
            PdfluentLicensing.setPublicKey(key);
            // Idempotent re-injection with the SAME key must be a no-op.
            PdfluentLicensing.setPublicKey(key);
        } catch (PdfluentLicenseException e) {
            assertEquals("E-LICENSE-INVALID", e.getCode());
        }
    }

    /**
     * A bogus signed payload must surface as a typed
     * {@link PdfluentLicenseException} (never a bare RuntimeException or an
     * {@link UnsatisfiedLinkError}). The exact code depends on whether a
     * public key is configured and on parse order, so we assert only the
     * typed family + that a non-null canonical code is present.
     */
    @Test
    void activatePayloadBogusThrowsTyped() {
        PdfluentLicenseException e = assertThrows(PdfluentLicenseException.class,
            () -> PdfluentLicensing.activatePayload("{ this is not valid license json "));
        assertNotNull(e.getCode());
        assertTrue(e.getCode().startsWith("E-LICENSE-"),
            "expected a canonical E-LICENSE-* code, got: " + e.getCode());
    }

    // ---------------------------------------------------------------------
    // Signatures: unsigned document
    // ---------------------------------------------------------------------

    @Test
    void signatureCountOnUnsignedDocIsZero() throws Exception {
        try (PdfDocument doc = openCorpus("simple.pdf")) {
            assertEquals(0, doc.signatureCount());
            assertTrue(doc.verifySignatures().isEmpty());
        }
    }

    @Test
    void isSignatureValidOutOfRangeThrows() throws Exception {
        try (PdfDocument doc = openCorpus("simple.pdf")) {
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.isSignatureValid(0));
        }
    }

    @Test
    void signatureMethodsBoundOnMultiPage() throws Exception {
        // Proves pdf_signature_count / pdf_signature_is_valid are bound:
        // calling them on a real multi-page doc must not raise
        // UnsatisfiedLinkError, and must return a sane (>= 0) count.
        try (PdfDocument doc = openCorpus("multi-page.pdf")) {
            assertTrue(doc.signatureCount() >= 0);
            assertNotNull(doc.verifySignatures());
        }
    }

    @Test
    void usingClosedDocForSignatureCountThrows() throws Exception {
        PdfDocument doc = openCorpus("simple.pdf");
        doc.close();
        assertThrows(IllegalStateException.class, doc::signatureCount);
    }

    // ---------------------------------------------------------------------
    // Signing: PKCS#12 round-trip + typed error mapping
    // ---------------------------------------------------------------------

    @Test
    void signNullPathThrows() throws Exception {
        try (PdfDocument doc = openCorpus("simple.pdf")) {
            assertThrows(NullPointerException.class, () -> doc.sign(null, "x"));
        }
    }

    @Test
    void signMissingBundleThrowsIoException() throws Exception {
        try (PdfDocument doc = openCorpus("simple.pdf")) {
            assertThrows(PdfluentIoException.class,
                () -> doc.sign("/nonexistent/never-exists.p12", "x"));
        }
    }

    @Test
    void signWrongPasswordThrowsValidation() throws Exception {
        org.junit.jupiter.api.Assumptions.assumeTrue(
            Files.exists(RSA_P12), "RSA PKCS#12 fixture not present: " + RSA_P12);
        try (PdfDocument doc = openCorpus("simple.pdf")) {
            assertThrows(PdfluentValidationException.class,
                () -> doc.sign(RSA_P12.toString(), "definitely-wrong"));
        }
    }

    /**
     * Full sign → verify round-trip: signing an unsigned document with the RSA
     * PKCS#12 identity yields a NEW document with exactly one VALID signature.
     */
    @Test
    void signThenVerifyRoundTrip() throws Exception {
        org.junit.jupiter.api.Assumptions.assumeTrue(
            Files.exists(RSA_P12), "RSA PKCS#12 fixture not present: " + RSA_P12);
        try (PdfDocument src = openCorpus("simple.pdf")) {
            assertEquals(0, src.signatureCount(), "source must start unsigned");
            try (PdfDocument signed = src.sign(RSA_P12.toString(), RSA_P12_PASSWORD)) {
                assertEquals(1, signed.signatureCount());
                assertTrue(signed.isSignatureValid(0));

                List<PdfDocument.SignatureValidation> results = signed.verifySignatures();
                assertEquals(1, results.size());
                PdfDocument.SignatureValidation r = results.get(0);
                assertEquals(0, r.index());
                assertEquals(PdfDocument.SignatureStatus.VALID, r.status());
                assertTrue(r.isValid());
            }
            // The source document is unchanged by signing.
            assertEquals(0, src.signatureCount());
        }
    }
}
