package com.pdfluent;

/**
 * Minimal test-scoped shim that exposes the JNI native methods from
 * {@code libpdfluent_java} for the signed-licence lifecycle tests.
 *
 * <p>This class exists only in the {@code test} source set and is never
 * shipped.  Production callers should use the canonical JNA path in
 * {@code bindings/java} ({@code com.pdfluent.PdfluentLicensing} backed by
 * {@code libpdf_capi}).  This shim exercises the <em>JNI</em> symbols from
 * {@code pdf-java/src/lib.rs} which are the layer being validated here.
 *
 * <p>The library is loaded from {@code java.library.path} which Maven
 * Surefire configures via {@code argLine=-Djava.library.path=../../target/release}.
 */
public final class PdfluentLicensing {

    static {
        System.loadLibrary("pdfluent_java");
    }

    private PdfluentLicensing() {}

    // -----------------------------------------------------------------------
    // JNI native method declarations — bound to Java_com_pdfluent_PdfluentLicensing_*
    // symbols in libpdfluent_java.
    // -----------------------------------------------------------------------

    /** Activate a 1.0 evaluation key ({@code "tier:<name>"}). */
    static native void nativeActivateKey(String key);

    /**
     * Configure the 32-byte Ed25519 public key for signed-payload verification.
     * Must be called before {@link #nativeActivatePayload(String)}.
     */
    static native void nativeSetPublicKey(byte[] key);

    /**
     * Activate a cryptographically-signed JSON license payload (SDK 1.1+).
     * Requires a prior call to {@link #nativeSetPublicKey(byte[])}.
     */
    static native void nativeActivatePayload(String payloadJson);

    /** Return the effective tier as an integer (0=Trial … 4=Enterprise). */
    static native int nativeEffectiveTier();

    // -----------------------------------------------------------------------
    // Thin public wrappers — map JNI throw to unchecked Java exceptions.
    // -----------------------------------------------------------------------

    /**
     * Activate a 1.0 evaluation key.
     *
     * @param key e.g. {@code "tier:developer"}
     * @throws PdfluentLicenseException on invalid key or conflicting tier
     */
    public static void activateKey(String key) {
        if (key == null) throw new NullPointerException("key");
        nativeActivateKey(key);
    }

    /**
     * Configure the Ed25519 public key for signed-payload verification.
     *
     * @param key exactly 32 raw bytes
     * @throws PdfluentLicenseException if the key length is wrong or a
     *         different key was already configured
     */
    public static void setPublicKey(byte[] key) {
        if (key == null) throw new NullPointerException("key");
        nativeSetPublicKey(key);
    }

    /**
     * Activate a signed JSON license payload.
     *
     * @param payloadJson JSON produced by the PDFluent licence-generator
     * @throws PdfluentLicenseException on invalid/expired/wrong-signature payload
     */
    public static void activatePayload(String payloadJson) {
        if (payloadJson == null) throw new NullPointerException("payloadJson");
        nativeActivatePayload(payloadJson);
    }

    /** Return the effective tier integer (0=Trial, 1=Developer, …, 4=Enterprise). */
    public static int effectiveTier() {
        return nativeEffectiveTier();
    }
}
