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

            // Tier conflict — surfaced as a typed PdfluentLicenseException
            // with the canonical C8 code, matching the .NET / Node /
            // Python / WASM bindings.
            PdfluentLicenseException e = assertThrows(PdfluentLicenseException.class,
                () -> PdfluentLicensing.activateKey("tier:enterprise"));
            assertEquals("E-LICENSE-INVALID", e.getCode());
        } catch (PdfluentLicenseException e) {
            // Another test activated to a different tier first; the test
            // is shape-only here — `e` was raised by the FIRST activate
            // call in this method because the process is already locked
            // to a different tier.  Verify the typed code surface holds.
            assertEquals("E-LICENSE-INVALID", e.getCode());
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
            } catch (PdfluentLicenseException e) {
                // Already activated to a different tier by another test;
                // accept ONLY the canonical typed surface (E-LICENSE-INVALID).
                assertEquals("E-LICENSE-INVALID", e.getCode());
            }
        } finally {
            Files.deleteIfExists(tmp);
        }
    }

    /**
     * Phase-3 fix for GAP-L01 — pins the typed-code surface of the
     * STATUS_LICENSE_ALREADY_SET case.  Every other binding surfaces this
     * as a typed PdfluentLicenseException-equivalent with
     * {@code code == "E-LICENSE-INVALID"}; this test asserts Java now
     * matches.
     *
     * <p>The test is shape-only: it executes the conflict path inside a
     * best-effort lifecycle and asserts ONLY that the exception type and
     * code match.  It does not require any specific tier to be active.
     */
    @Test
    void activateKeyAlreadySetThrowsPdfluentLicenseExceptionWithCanonicalCode() {
        // Try to drive the process into the "already set" arm.  If this
        // test runs first in the JVM, the first activate succeeds and the
        // second raises with a tier conflict.  If a prior test already
        // activated, the first activate here raises with a tier conflict.
        // Either way, AT LEAST ONE of the two activate calls below must
        // raise a PdfluentLicenseException with code E-LICENSE-INVALID
        // (the typed-code-parity contract).
        boolean sawTypedConflict = false;
        try {
            PdfluentLicensing.activateKey("tier:developer");
        } catch (PdfluentLicenseException e) {
            assertEquals("E-LICENSE-INVALID", e.getCode());
            sawTypedConflict = true;
        }
        try {
            PdfluentLicensing.activateKey("tier:enterprise");
        } catch (PdfluentLicenseException e) {
            assertEquals("E-LICENSE-INVALID", e.getCode());
            sawTypedConflict = true;
        }
        assertTrue(sawTypedConflict,
            "expected at least one activate() to raise PdfluentLicenseException"
            + " with code E-LICENSE-INVALID across the two divergent tiers");
    }
}
