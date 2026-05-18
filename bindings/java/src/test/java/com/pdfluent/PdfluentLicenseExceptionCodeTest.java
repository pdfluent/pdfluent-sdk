package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.ObjectInputStream;
import java.io.ObjectOutputStream;

import static org.junit.jupiter.api.Assertions.*;

/**
 * JUnit 5 tests for the canonical C8 error code exposed by
 * {@link PdfluentException#getCode()}, with focus on
 * {@link PdfluentLicenseException}.
 *
 * <p>Closes the documented P3 gap (track
 * {@code JAVA_LICENSE_EXCEPTION_CODE_FIELD}): Java now exposes the canonical
 * C8 code as a structured getter, matching the .NET
 * {@code PdfluentLicenseException.Code} property and the Python
 * {@code PdfluentLicenseError.code} attribute.
 */
class PdfluentLicenseExceptionCodeTest {

    /**
     * activateKey with a bad license string surfaces a typed
     * {@link PdfluentLicenseException} whose {@link PdfluentException#getCode()}
     * equals {@code "E-LICENSE-INVALID"} — the canonical C8 catalogue code.
     */
    @Test
    void licenseInvalidKeyHasCode() {
        PdfluentLicenseException ex = assertThrows(
            PdfluentLicenseException.class,
            () -> PdfluentLicensing.activateKey("totally-not-a-license"));
        assertEquals("E-LICENSE-INVALID", ex.getCode(),
            "PdfluentLicenseException must carry the canonical C8 code via getCode()");
    }

    /**
     * The {@code (String, String)} constructor on {@link PdfluentLicenseException}
     * round-trips the canonical code, independent of any native code path —
     * locks the API surface for callers that catch and re-throw with their
     * own code mapping.
     */
    @Test
    void codeStableAcrossNativeRoutes() {
        // Native (JNA) route — driven through the licensing API:
        PdfluentLicenseException native_ = assertThrows(
            PdfluentLicenseException.class,
            () -> PdfluentLicensing.activateKey("tier:platinum"));
        assertEquals("E-LICENSE-INVALID", native_.getCode());

        // Direct constructor route — simulates the JNI-side
        // throw_pdf_exception_with_code path which uses the same
        // (String, String) constructor.
        PdfluentLicenseException direct =
            new PdfluentLicenseException("synthetic", "E-LICENSE-INVALID");
        assertEquals("E-LICENSE-INVALID", direct.getCode());
        assertEquals("synthetic", direct.getMessage());

        // Both routes must produce string-equal codes.
        assertEquals(native_.getCode(), direct.getCode());
    }

    /**
     * The pre-existing single-arg and (String, Throwable) constructors must
     * continue to work and must return {@code null} from {@link
     * PdfluentException#getCode()} — no breaking change for callers built
     * against earlier 0.x versions.
     */
    @Test
    void backwardsCompatOldConstructor() {
        PdfluentLicenseException one = new PdfluentLicenseException("msg only");
        assertEquals("msg only", one.getMessage());
        assertNull(one.getCode(),
            "Legacy single-arg constructor must leave code() as null");

        Throwable cause = new RuntimeException("root");
        PdfluentLicenseException two = new PdfluentLicenseException("with cause", cause);
        assertEquals("with cause", two.getMessage());
        assertSame(cause, two.getCause());
        assertNull(two.getCode(),
            "Legacy (String, Throwable) constructor must leave code() as null");

        // Base class shares the contract.
        PdfluentException base = new PdfluentException("plain");
        assertNull(base.getCode());
    }

    /**
     * A {@link PdfluentLicenseException} carrying a code must survive
     * Java serialization round-trips with the {@code code} field intact.
     * Catches accidental {@code transient} markings on the field.
     */
    @Test
    void codeSerializable() throws IOException, ClassNotFoundException {
        PdfluentLicenseException original =
            new PdfluentLicenseException("invalid: bad-format", "E-LICENSE-INVALID");

        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        try (ObjectOutputStream oos = new ObjectOutputStream(baos)) {
            oos.writeObject(original);
        }
        Object copy;
        try (ObjectInputStream ois = new ObjectInputStream(
                new ByteArrayInputStream(baos.toByteArray()))) {
            copy = ois.readObject();
        }
        assertNotNull(copy);
        assertTrue(copy instanceof PdfluentLicenseException);
        PdfluentLicenseException restored = (PdfluentLicenseException) copy;
        assertEquals("invalid: bad-format", restored.getMessage());
        assertEquals("E-LICENSE-INVALID", restored.getCode(),
            "code must survive Java serialization");
    }

    /**
     * Demonstrates the canonical code-branching idiom from the docs — callers
     * can switch on {@code getCode()} without resorting to message-string
     * parsing.
     */
    @Test
    void codeBranchingIdiom() {
        try {
            PdfluentLicensing.activateKey("totally-not-a-license");
            fail("expected PdfluentLicenseException");
        } catch (PdfluentLicenseException e) {
            String code = e.getCode();
            if ("E-LICENSE-INVALID".equals(code)) {
                // The branch we care about.
                assertNotNull(e.getMessage());
                return;
            }
            fail("unexpected code: " + code);
        }
    }

    /**
     * Subclass constructors that take a code must round-trip the code through
     * the base class. Spot-check {@link PdfluentParseException} as a
     * representative non-license subclass.
     */
    @Test
    void subclassCodeConstructors() {
        PdfluentParseException p =
            new PdfluentParseException("bad bytes", "E-PARSE-INVALID-PDF");
        assertEquals("E-PARSE-INVALID-PDF", p.getCode());
        assertTrue(p instanceof PdfluentException);

        PdfluentIoException io =
            new PdfluentIoException("disk full", "E-IO-GENERIC");
        assertEquals("E-IO-GENERIC", io.getCode());
    }
}
