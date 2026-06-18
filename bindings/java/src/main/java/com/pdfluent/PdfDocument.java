package com.pdfluent;

import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.PointerByReference;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/**
 * A PDF document backed by the native PDFluent engine.
 *
 * <p>Use with try-with-resources to ensure the native handle is always freed:
 * <pre>{@code
 * try (PdfDocument doc = PdfDocument.open(Path.of("invoice.pdf"))) {
 *     System.out.println("Pages: " + doc.getPageCount());
 * }
 * }</pre>
 */
public final class PdfDocument implements AutoCloseable {

    private static final PdfCapiLibrary LIB = NativeLoader.get();

    private volatile Pointer handle;

    private PdfDocument(Pointer handle) {
        this.handle = handle;
    }

    // --- Factory methods ---

    /**
     * Open a PDF from a file path.
     *
     * @param path path to the PDF file
     * @throws PdfluentException if the file cannot be read or is not a valid PDF
     */
    public static PdfDocument open(Path path) {
        byte[] bytes;
        try {
            bytes = Files.readAllBytes(path);
        } catch (IOException e) {
            throw new PdfluentException("cannot read file: " + e.getMessage(), e);
        }
        return open(bytes);
    }

    /**
     * Open a PDF from raw bytes.
     *
     * @param data PDF file contents
     * @throws PdfluentException if the data is not a valid PDF
     */
    public static PdfDocument open(byte[] data) {
        if (data == null) throw new IllegalArgumentException("data must not be null");
        PointerByReference out = new PointerByReference();
        int status = LIB.pdf_document_open_from_bytes(data, new NativeLong(data.length), out);
        if (status != 0) {
            throw new PdfluentException(lastError());
        }
        Pointer handle = out.getValue();
        if (handle == null || handle.equals(Pointer.NULL)) {
            throw new PdfluentException("open returned null handle");
        }
        return new PdfDocument(handle);
    }

    // --- Properties ---

    /** Number of pages in the document. */
    public int getPageCount() {
        return LIB.pdf_document_page_count(requireOpen());
    }

    // --- Content ---

    /**
     * Extract plain text from a page.
     *
     * @param pageIndex zero-based page index
     * @return extracted text, or {@code ""} if the page has no text
     * @throws PdfluentException if the page index is out of range
     */
    public String extractText(int pageIndex) {
        Pointer ptr = LIB.pdf_page_extract_text(requireOpen(), pageIndex);
        if (ptr == null || ptr.equals(Pointer.NULL)) {
            String err = lastError();
            if (!err.isEmpty()) throw new PdfluentException(err);
            return "";
        }
        try {
            return ptr.getString(0, StandardCharsets.UTF_8.name());
        } finally {
            LIB.pdf_string_free(ptr);
        }
    }

    // --- Digital signatures ---

    /**
     * Sign this document with a PKCS#12 identity bundle, returning a NEW signed
     * document.
     *
     * <p>The source document is not modified. The returned {@link PdfDocument}
     * owns its own native handle and must be closed independently (e.g. with
     * try-with-resources).
     *
     * @param pkcs12Path     path to a {@code .p12} / {@code .pfx} bundle; must
     *                       not be {@code null}
     * @param pkcs12Password password for the bundle, or {@code null} for a
     *                       password-less bundle
     * @return a new, open signed {@code PdfDocument}
     * @throws PdfluentIoException   if the PKCS#12 file does not exist
     * @throws PdfluentValidationException if the bundle is malformed or signing
     *                               fails in the engine
     * @throws PdfluentException     for any other engine failure
     * @throws IllegalStateException if this document has been closed
     */
    public PdfDocument sign(String pkcs12Path, String pkcs12Password) {
        Objects.requireNonNull(pkcs12Path, "pkcs12Path");
        Pointer h = requireOpen();
        PointerByReference out = new PointerByReference();
        int status = LIB.pdf_document_sign(h, pkcs12Path, pkcs12Password, out);
        if (status != STATUS_OK) {
            throw signError(status, pkcs12Path);
        }
        Pointer signed = out.getValue();
        if (signed == null || signed.equals(Pointer.NULL)) {
            throw new PdfluentException("sign returned a null document handle");
        }
        return new PdfDocument(signed);
    }

    /**
     * Number of signature fields present in this document.
     *
     * @return signature count ({@code >= 0}); {@code 0} when the document has
     *         no signature fields
     * @throws IllegalStateException if this document has been closed
     */
    public int signatureCount() {
        int count = LIB.pdf_signature_count(requireOpen());
        return Math.max(count, 0);
    }

    /**
     * Validate the digital signature at zero-based {@code index}.
     *
     * @param index zero-based signature index in {@code [0, signatureCount())}
     * @return {@code true} if the signature is cryptographically valid;
     *         {@code false} if it is invalid or its status cannot be determined
     * @throws PdfluentPageRangeException if {@code index} is out of range
     * @throws IllegalStateException      if this document has been closed
     */
    public boolean isSignatureValid(int index) {
        Pointer h = requireOpen();
        int count = LIB.pdf_signature_count(h);
        if (index < 0 || index >= count) {
            throw new PdfluentPageRangeException(
                "signature index " + index + " is out of range [0, " + Math.max(count, 0) + ")");
        }
        return LIB.pdf_signature_is_valid(h, index) == 1;
    }

    /**
     * Validate every digital signature in the document and return a structured
     * report, one {@link SignatureValidation} per signature field, ordered by
     * index.
     *
     * <p>Mirrors the structured {@code validateSignatures()} surface of the
     * Node, .NET and Python bindings. An empty list means the document has no
     * signature fields.
     *
     * @return an immutable, index-ordered list of per-signature results; never
     *         {@code null}
     * @throws IllegalStateException if this document has been closed
     */
    public List<SignatureValidation> verifySignatures() {
        Pointer h = requireOpen();
        int count = LIB.pdf_signature_count(h);
        if (count <= 0) {
            return Collections.emptyList();
        }
        List<SignatureValidation> results = new ArrayList<>(count);
        for (int i = 0; i < count; i++) {
            int raw = LIB.pdf_signature_is_valid(h, i);
            results.add(new SignatureValidation(i, SignatureStatus.fromNative(raw)));
        }
        return Collections.unmodifiableList(results);
    }

    // --- Lifecycle ---

    /** Close the document and free native resources. Safe to call multiple times. */
    @Override
    public void close() {
        Pointer h = handle;
        if (h != null) {
            handle = null;
            LIB.pdf_document_free(h);
        }
    }

    /** True if the document has not been closed yet. */
    public boolean isOpen() {
        return handle != null;
    }

    // --- Helpers ---

    private Pointer requireOpen() {
        Pointer h = handle;
        if (h == null) throw new IllegalStateException("PdfDocument is closed");
        return h;
    }

    private static String lastError() {
        String err = LIB.pdf_get_last_error();
        return (err != null && !err.isEmpty()) ? err : "native call failed (no error message)";
    }

    /** PdfStatus success sentinel (PDF_STATUS_OK). */
    private static final int STATUS_OK = 0;
    /** PDF_STATUS_ERROR_FILE_NOT_FOUND. */
    private static final int STATUS_FILE_NOT_FOUND = 2;
    /** PDF_STATUS_ERROR_SIGN. */
    private static final int STATUS_SIGN = 9;

    /**
     * Map a non-OK {@code pdf_document_sign} status to the most specific typed
     * {@link PdfluentException} subclass.
     */
    private static PdfluentException signError(int status, String pkcs12Path) {
        switch (status) {
            case STATUS_FILE_NOT_FOUND:
                return new PdfluentIoException(
                    "PKCS#12 bundle not found: '" + pkcs12Path + "': " + lastError());
            case STATUS_SIGN:
                return new PdfluentValidationException(
                    "signing failed: " + lastError());
            default:
                return new PdfluentException(
                    "signing failed (status " + status + "): " + lastError());
        }
    }

    // --- Signature result types ---

    /**
     * Cryptographic validation status of a single signature, mirroring the
     * {@code valid} / {@code invalid} / {@code unknown} states surfaced by the
     * Node, .NET and Python bindings.
     */
    public enum SignatureStatus {
        /** The signature is cryptographically valid. */
        VALID,
        /** The signature is present but does not verify. */
        INVALID,
        /** The status could not be determined (e.g. unsupported algorithm). */
        UNKNOWN;

        /**
         * Map the native {@code pdf_signature_is_valid} return value
         * ({@code 1} / {@code 0} / {@code -1}) to a {@code SignatureStatus}.
         *
         * @param raw native tri-state result
         * @return the corresponding {@code SignatureStatus}
         */
        static SignatureStatus fromNative(int raw) {
            if (raw == 1) return VALID;
            if (raw == 0) return INVALID;
            return UNKNOWN;
        }
    }

    /**
     * Immutable result of validating one signature field, returned by
     * {@link PdfDocument#verifySignatures()}.
     */
    public static final class SignatureValidation {
        private final int index;
        private final SignatureStatus status;

        SignatureValidation(int index, SignatureStatus status) {
            this.index = index;
            this.status = Objects.requireNonNull(status, "status");
        }

        /** Zero-based index of the signature field this result describes. */
        public int index() {
            return index;
        }

        /** Cryptographic validation status of the signature. */
        public SignatureStatus status() {
            return status;
        }

        /** {@code true} iff {@link #status()} is {@link SignatureStatus#VALID}. */
        public boolean isValid() {
            return status == SignatureStatus.VALID;
        }

        @Override
        public String toString() {
            return "SignatureValidation(index=" + index + ", status=" + status + ")";
        }
    }
}
