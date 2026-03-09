package com.xfa.pdf

/**
 * Exception thrown when a PDF operation fails in the native engine.
 */
class PdfException : RuntimeException {
    constructor(message: String) : super(message)
    constructor(message: String, cause: Throwable) : super(message, cause)
}
