// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

package com.pdfluent;

/**
 * Test-scoped shim over the document half of {@code libpdfluent_java}.
 *
 * <p>The JNI symbols in {@code crates/pdf-java/src/lib.rs} are named for
 * {@code com.pdfluent.PdfluentDocument} (since 18742a72), so this is the class
 * name and package the JVM needs in order to bind them. The shipped wrapper
 * around these symbols lives in {@code bindings/java}; this class exists only
 * so the crate's own suite can call the JNI layer directly, the same way
 * {@link PdfluentLicensing} does for the licence half. Never shipped.
 *
 * <p>Only the natives the suite exercises are declared. Declaring one the
 * library does not export would not fail at compile time, only at first call,
 * so keep this list to what {@code PdfluentDocumentJniTest} uses.
 */
final class PdfluentDocument {

    static {
        System.loadLibrary("pdfluent_java");
    }

    private PdfluentDocument() {}

    /** Open from bytes; returns a handle, or throws {@link PdfluentParseException}. */
    static native long nativeOpen(byte[] data);

    /** Release the handle. A zero handle is a no-op. */
    static native void nativeClose(long handle);

    /** Page count; zero for a zero handle. */
    static native int nativePageCount(long handle);

    /** Text of one page; throws {@link PdfluentException} on a zero handle. */
    static native String nativeExtractText(long handle, int pageIndex);

    /** One Info-dictionary entry, or {@code null} when the key is absent. */
    static native String nativeGetMetadata(long handle, String key);
}
