package com.xfa.pdf;

import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * JUnit tests for {@link PdfluentLicensing}.
 *
 * <p>Requires libpdf_java to be loadable; see PdfDocumentTest for setup.
 * Tests use only fake-format keys. The Rust core's process-global tier means
 * lifecycle tests tolerate the case where another test activated first.
 */
class PdfluentLicensingTest {

    @Test
    void statusHasKnownShape() {
        PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
        assertNotNull(s.tier);
        assertNotNull(s.source);
        // outputIsMarked is bool — nothing to assert beyond shape.
    }

    @Test
    void effectiveTierIsInRange() {
        PdfluentLicensing.Tier t = PdfluentLicensing.effectiveTier();
        assertNotNull(t);
    }

    @Test
    void activateKeyNullThrows() {
        assertThrows(NullPointerException.class,
            () -> PdfluentLicensing.activateKey(null));
    }

    @Test
    void activateKeyInvalidThrowsPdfException() {
        assertThrows(PdfException.class,
            () -> PdfluentLicensing.activateKey("totally-not-a-license"));
    }

    @Test
    void activateKeyUnknownTierThrows() {
        assertThrows(PdfException.class,
            () -> PdfluentLicensing.activateKey("tier:platinum"));
    }

    @Test
    void activateFileMissingPathThrowsIoException() {
        assertThrows(IOException.class,
            () -> PdfluentLicensing.activateFile("/nonexistent/never-exists.lic"));
    }

    @Test
    void activationLifecycle() {
        try {
            PdfluentLicensing.activateKey("tier:developer");
            PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
            assertEquals(PdfluentLicensing.Tier.DEVELOPER, s.tier);
            assertEquals(PdfluentLicensing.Source.EXPLICIT, s.source);
            assertFalse(s.outputIsMarked);

            // Idempotent re-activate
            PdfluentLicensing.activateKey("tier:developer");

            // Conflict
            assertThrows(IllegalStateException.class,
                () -> PdfluentLicensing.activateKey("tier:enterprise"));
        } catch (IllegalStateException e) {
            // Another test activated first; ok.
        }
    }

    @Test
    void activateFileReadsKey() throws Exception {
        Path tmp = Files.createTempFile("pdfluent-fake-", ".lic");
        Files.writeString(tmp, "tier:team\n");
        try {
            try {
                PdfluentLicensing.activateFile(tmp.toString());
                PdfluentLicensing.Tier t = PdfluentLicensing.effectiveTier();
                assertTrue(t == PdfluentLicensing.Tier.TEAM
                        || t == PdfluentLicensing.Tier.DEVELOPER
                        || t == PdfluentLicensing.Tier.BUSINESS
                        || t == PdfluentLicensing.Tier.ENTERPRISE);
            } catch (IllegalStateException ignored) {
                // Already activated — ok.
            }
        } finally {
            Files.deleteIfExists(tmp);
        }
    }
}
