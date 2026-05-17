package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * JUnit tests for {@link PdfluentLicensing}.
 *
 * <p>Requires {@code libpdf_capi} to be loadable; classpath bundles it
 * via {@code /native/<arch>/}. Tests use fake-format keys only.
 */
class PdfluentLicensingTest {

    @Test
    void statusHasKnownShape() {
        PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
        assertNotNull(s.tier);
        assertNotNull(s.source);
    }

    @Test
    void effectiveTierIsAnEnum() {
        PdfluentLicensing.Tier t = PdfluentLicensing.effectiveTier();
        assertNotNull(t);
    }

    @Test
    void activateKeyNullThrows() {
        assertThrows(NullPointerException.class,
            () -> PdfluentLicensing.activateKey(null));
    }

    @Test
    void activateKeyInvalidThrowsPdfluentException() {
        assertThrows(PdfluentException.class,
            () -> PdfluentLicensing.activateKey("totally-not-a-license"));
    }

    @Test
    void activateKeyUnknownTierThrows() {
        assertThrows(PdfluentException.class,
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
                assertNotEquals(PdfluentLicensing.Tier.TRIAL, t);
            } catch (IllegalStateException ignored) {
                // Already activated — ok.
            }
        } finally {
            Files.deleteIfExists(tmp);
        }
    }
}
