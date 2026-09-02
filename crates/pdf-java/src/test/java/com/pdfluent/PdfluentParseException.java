// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

package com.pdfluent;

/**
 * Test-scoped shim for the exception {@code nativeOpen} raises on bytes that
 * are not a PDF. The native layer calls {@code ThrowNew} with this class name,
 * so the JVM needs a class of exactly this name on the classpath -- otherwise
 * the test would see a {@code NoClassDefFoundError} instead of the failure it
 * is asserting. Mirrors {@code bindings/java}'s class of the same name.
 */
public class PdfluentParseException extends PdfluentException {
    private static final long serialVersionUID = 2L;

    public PdfluentParseException(String message) {
        super(message);
    }

    public PdfluentParseException(String message, String code) {
        super(message, code);
    }
}
