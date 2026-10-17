// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.NativeLongByReference;
import com.sun.jna.ptr.PointerByReference;

import java.nio.charset.StandardCharsets;
import java.util.Objects;

/**
 * Shared plumbing for the compatible API (lane L5): status checks, error
 * mapping, and native memory readers. Package-private; the public classes
 * delegate here so the JNA call shape stays uniform.
 */
final class CompatSupport {

    private static final PdfCapiLibrary LIB = NativeLoader.get();

    private CompatSupport() {}

    static PdfCapiLibrary lib() {
        return LIB;
    }

    /** Throw the mapped exception when a status code signals failure. */
    static void check(int status, String what) {
        if (status != 0) {
            throw error(status, what);
        }
    }

    /** Build the most specific exception for a (status, kind) pair. */
    static PdfluentException error(int status, String what) {
        String message = LIB.pdf_get_last_error();
        if (message == null || message.isEmpty()) {
            message = what + " failed";
        }
        switch (LIB.pdf_compat_last_error_kind()) {
            case 7:
                return new EmptyFileError(message);
            case 6:
                return new FileDataError(message);
            case 5:
                return new FileNotFoundError(message);
            case 10:
                return new FitzDeprecation(message);
            default:
                break;
        }
        switch (status) {
            case 1:
                return new PdfluentValidationException(message);
            case 2:
                return new PdfluentIoException(message);
            case 3:
                return new PdfluentEncryptedDocumentException(message);
            case 4:
                return new PdfluentParseException(message);
            case 5:
                return new PdfluentPageRangeException(message);
            case 6:
                return new PdfluentRenderException(message);
            default:
                return new PdfluentException(message);
        }
    }

    /** Read a heap C string and free it. */
    static String readString(PointerByReference out) {
        Pointer ptr = out.getValue();
        if (ptr == null || ptr.equals(Pointer.NULL)) {
            return "";
        }
        try {
            return ptr.getString(0, StandardCharsets.UTF_8.name());
        } finally {
            LIB.pdf_string_free(ptr);
        }
    }

    /** Read heap bytes and free them. */
    static byte[] readBytes(PointerByReference data, NativeLongByReference len) {
        Pointer ptr = data.getValue();
        long count = len.getValue().longValue();
        if (ptr == null || ptr.equals(Pointer.NULL) || count == 0) {
            if (ptr != null && !ptr.equals(Pointer.NULL)) {
                LIB.pdf_bytes_free(ptr, new NativeLong(0));
            }
            return new byte[0];
        }
        try {
            return ptr.getByteArray(0, (int) count);
        } finally {
            LIB.pdf_bytes_free(ptr, new NativeLong(count));
        }
    }

    /** Read heap ints and free them. */
    static int[] readInts(PointerByReference data, NativeLongByReference len) {
        Pointer ptr = data.getValue();
        long count = len.getValue().longValue();
        if (ptr == null || ptr.equals(Pointer.NULL) || count == 0) {
            if (ptr != null && !ptr.equals(Pointer.NULL)) {
                LIB.pdf_bytes_free(ptr, new NativeLong(0));
            }
            return new int[0];
        }
        try {
            return ptr.getIntArray(0, (int) count);
        } finally {
            LIB.pdf_bytes_free(ptr, new NativeLong(count * 4));
        }
    }

    /** Read heap doubles and free them. */
    static double[] readDoubles(PointerByReference data, NativeLongByReference len) {
        Pointer ptr = data.getValue();
        long count = len.getValue().longValue();
        if (ptr == null || ptr.equals(Pointer.NULL) || count == 0) {
            if (ptr != null && !ptr.equals(Pointer.NULL)) {
                LIB.pdf_bytes_free(ptr, new NativeLong(0));
            }
            return new double[0];
        }
        try {
            return ptr.getDoubleArray(0, (int) count);
        } finally {
            LIB.pdf_bytes_free(ptr, new NativeLong(count * 8));
        }
    }

    static int flag(boolean value) {
        return value ? 1 : 0;
    }

    static NativeLong countOf(byte[] values) {
        return new NativeLong(values == null ? 0 : values.length);
    }

    static NativeLong countOf(int[] values) {
        return new NativeLong(values == null ? 0 : values.length);
    }

    static NativeLong countOf(double[] values) {
        return new NativeLong(values == null ? 0 : values.length);
    }

    static NativeLong countOf(String[] values) {
        return new NativeLong(values == null ? 0 : values.length);
    }

    static NativeLong countOf(PdfCapiLibrary.PdfCompatQuad[] values) {
        return new NativeLong(values == null ? 0 : values.length);
    }

    static <T> T require(T value, String name) {
        return Objects.requireNonNull(value, name);
    }

    /** Quote a string for the options JSON wire format. */
    static String jsonQuote(String value) {
        StringBuilder out = new StringBuilder(value.length() + 2);
        out.append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            switch (c) {
                case '"':
                    out.append("\"");
                    break;
                case '\\':
                    out.append("\\");
                    break;
                case '\n':
                    out.append("\n");
                    break;
                case '\r':
                    out.append("\r");
                    break;
                case '\t':
                    out.append("\t");
                    break;
                default:
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                    break;
            }
        }
        out.append('"');
        return out.toString();
    }

    /** Render doubles for the options JSON wire format. */
    static String jsonDoubles(double[] values) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < values.length; i++) {
            if (i > 0) {
                out.append(',');
            }
            out.append(values[i]);
        }
        return out.append(']').toString();
    }
}
